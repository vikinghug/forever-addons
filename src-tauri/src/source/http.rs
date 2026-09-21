//! One HTTP client for every source, with source-tagged errors.

use std::time::Duration;

use crate::domain::SourceId;
use crate::error::{AppError, Result};

/// Sent on every request, including archive downloads, so sources see one
/// consistent, honest client identity.
pub const USER_AGENT: &str = concat!(
    "forever-addons/",
    env!("CARGO_PKG_VERSION"),
    " (WoW Forever addon manager)"
);

#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        let inner = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|err| AppError::SourceRequest {
                source_id: SourceId::CurseForge,
                url: "(client construction)".to_owned(),
                reason: err.to_string(),
            })?;

        Ok(Self { inner })
    }

    pub fn raw(&self) -> &reqwest::Client {
        &self.inner
    }

    pub async fn get_text(&self, source: SourceId, url: &str) -> Result<String> {
        let response = self.send(source, self.inner.get(url), url).await?;
        response
            .text()
            .await
            .map_err(|err| request_failed(source, url, &err))
    }

    /// Also hands back the response headers, which is how paginated WordPress
    /// endpoints report their total page count.
    pub async fn get_json_page(
        &self,
        source: SourceId,
        url: &str,
    ) -> Result<(serde_json::Value, Option<u32>)> {
        let response = self.send(source, self.inner.get(url), url).await?;
        let total_pages = header_number(&response, "x-wp-totalpages");

        let body = response
            .text()
            .await
            .map_err(|err| request_failed(source, url, &err))?;

        let json = serde_json::from_str(&body).map_err(|err| AppError::SourceShape {
            source_id: source,
            url: url.to_owned(),
            expected: "a JSON body",
            reason: err.to_string(),
        })?;

        Ok((json, total_pages))
    }

    /// A JSON GET carrying extra request headers — how the CurseForge API
    /// takes its `x-api-key`.
    pub async fn get_json_authed(
        &self,
        source: SourceId,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<serde_json::Value> {
        let mut request = self.inner.get(url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }

        let response = self.send(source, request, url).await?;
        let body = response
            .text()
            .await
            .map_err(|err| request_failed(source, url, &err))?;

        serde_json::from_str(&body).map_err(|err| AppError::SourceShape {
            source_id: source,
            url: url.to_owned(),
            expected: "a JSON body",
            reason: err.to_string(),
        })
    }

    pub async fn post_form(
        &self,
        source: SourceId,
        url: &str,
        form: &[(&str, &str)],
    ) -> Result<serde_json::Value> {
        let response = self
            .send(source, self.inner.post(url).form(form), url)
            .await?;
        let body = response
            .text()
            .await
            .map_err(|err| request_failed(source, url, &err))?;

        serde_json::from_str(&body).map_err(|err| AppError::SourceShape {
            source_id: source,
            url: url.to_owned(),
            expected: "a JSON body",
            reason: err.to_string(),
        })
    }

    async fn send(
        &self,
        source: SourceId,
        request: reqwest::RequestBuilder,
        url: &str,
    ) -> Result<reqwest::Response> {
        let response = request
            .send()
            .await
            .map_err(|err| request_failed(source, url, &err))?;

        let status = response.status();
        if !status.is_success() {
            return Err(AppError::SourceStatus {
                source_id: source,
                url: url.to_owned(),
                status: status.as_u16(),
            });
        }

        Ok(response)
    }
}

fn request_failed(source: SourceId, url: &str, err: &reqwest::Error) -> AppError {
    AppError::SourceRequest {
        source_id: source,
        url: url.to_owned(),
        reason: err.to_string(),
    }
}

fn header_number(response: &reqwest::Response, name: &str) -> Option<u32> {
    response
        .headers()
        .get(name)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}
