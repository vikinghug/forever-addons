//! Smoke tests against the real CurseForge, Wago, and GitHub services.
//!
//! These are `#[ignore]`d: they need the network, they are slow, and they fail
//! when a source changes its API — which is exactly what they are for. Run
//! them when a source's parsing looks wrong:
//!
//! ```sh
//! cargo test --test live_sources -- --ignored --nocapture
//! ```
//!
//! The CurseForge tests need `CURSEFORGE_API_KEY` (a key from
//! <https://console.curseforge.com>) and the Wago tests need `WAGO_TOKEN`
//! (from your Wago account settings) in the environment; without them they
//! skip with a note rather than fail.

use forever_addons_lib::domain::{Download, Expansion, SourceId};
use forever_addons_lib::source::{SourceAuth, Sources};

fn sources() -> Sources {
    Sources::new().expect("the HTTP client builds")
}

fn auth() -> SourceAuth {
    SourceAuth {
        curseforge_api_key: std::env::var("CURSEFORGE_API_KEY").ok(),
        wago_token: std::env::var("WAGO_TOKEN").ok(),
        github_repos: vec!["RevoltLive85/ForeverGuide".to_owned()],
    }
}

/// The source's whole listing, with progress ignored.
async fn catalog_of(source: SourceId) -> Vec<forever_addons_lib::domain::AddonSummary> {
    sources()
        .fetch_catalog(source, &auth(), &|_| {})
        .await
        .unwrap_or_else(|err| panic!("{source} catalog: {err}"))
}

#[tokio::test]
#[ignore = "hits api.curseforge.com"]
async fn curseforge_publishes_a_forever_catalog() {
    if auth().curseforge_api_key.is_none() {
        println!("CURSEFORGE_API_KEY is not set; skipping the CurseForge live test");
        return;
    }

    let addons = catalog_of(SourceId::CurseForge).await;

    assert!(
        addons.len() > 100,
        "expected several hundred Forever addons, got {}",
        addons.len()
    );
    assert!(
        addons
            .iter()
            .all(|addon| addon.expansions.contains(&Expansion::Forever)),
        "every catalog row is a Forever addon"
    );
    assert!(addons.iter().all(|addon| !addon.name.is_empty()));
    assert!(
        addons.iter().any(|addon| addon.is_installable()),
        "at least some mods allow API distribution"
    );
    assert!(
        addons
            .iter()
            .filter(|addon| addon.updated_at.is_some())
            .count()
            > addons.len() / 2,
        "CurseForge publishes file dates for most mods"
    );
}

#[tokio::test]
#[ignore = "hits api.curseforge.com"]
async fn curseforge_resolves_a_detail_and_a_download_url() {
    if auth().curseforge_api_key.is_none() {
        println!("CURSEFORGE_API_KEY is not set; skipping the CurseForge live test");
        return;
    }

    let addons = catalog_of(SourceId::CurseForge).await;
    let summary = addons
        .iter()
        .find(|addon| addon.is_installable())
        .expect("an installable Forever addon exists");

    let detail = sources()
        .fetch_detail(summary, &auth())
        .await
        .unwrap_or_else(|err| panic!("curseforge detail: {err}"));
    assert_eq!(detail.summary.id, summary.id);
    assert!(!detail.description.is_empty());

    let url = sources()
        .resolve_archive_url(summary, &auth())
        .await
        .unwrap_or_else(|err| panic!("curseforge download: {err}"));
    assert!(
        url.starts_with("https://"),
        "unexpected download URL: {url}"
    );
}

#[tokio::test]
#[ignore = "hits addons.wago.io"]
async fn wago_publishes_a_forever_catalog() {
    if auth().wago_token.is_none() {
        println!("WAGO_TOKEN is not set; skipping the Wago live test");
        return;
    }

    let addons = catalog_of(SourceId::Wago).await;

    assert!(
        !addons.is_empty(),
        "the popular listing lists at least something"
    );
    assert!(
        addons
            .iter()
            .all(|addon| addon.expansions.contains(&Expansion::Forever)),
        "every catalog row is a Forever addon"
    );
    assert!(
        addons
            .iter()
            .all(|addon| addon.download == Download::Brokered),
        "Wago links expire, so every download is brokered"
    );

    let summary = addons.first().expect("listing is not empty");
    let url = sources()
        .resolve_archive_url(summary, &auth())
        .await
        .unwrap_or_else(|err| panic!("wago download: {err}"));
    assert!(
        url.starts_with("https://"),
        "unexpected download URL: {url}"
    );
}

#[tokio::test]
#[ignore = "hits api.github.com"]
async fn github_catalogs_a_tracked_repository() {
    let addons = catalog_of(SourceId::GitHub).await;

    assert_eq!(addons.len(), 1);
    assert!(
        addons[0].id.to_string().starts_with("github:"),
        "{}",
        addons[0].id
    );
    assert!(
        addons[0].expansions.contains(&Expansion::Forever),
        "tracked repositories are Forever addons"
    );
}
