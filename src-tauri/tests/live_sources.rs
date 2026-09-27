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
//! (from <https://addons.wago.io/patreon>) in the environment; without them
//! they skip with a note rather than fail.

use forever_addons_lib::domain::{CatalogSource, Download, Expansion, LiveSource, Sort};
use forever_addons_lib::source::{SearchAccess, SourceAuth, Sources};

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
async fn catalog_of(source: CatalogSource) -> Vec<forever_addons_lib::domain::AddonSummary> {
    sources()
        .fetch_catalog(source, &auth(), &|_| {})
        .await
        .unwrap_or_else(|err| panic!("{source:?} catalog: {err}"))
}

fn has_curseforge_key() -> bool {
    let present = auth().curseforge_api_key.is_some();
    if !present {
        println!("CURSEFORGE_API_KEY is not set; skipping the CurseForge live test");
    }
    present
}

/// The browse list for `text`, starting from an untested key.
async fn curseforge_browse(text: &str) -> forever_addons_lib::source::LiveResults {
    sources()
        .browse_live(
            LiveSource::CurseForge,
            text,
            Sort::default(),
            SearchAccess::Untested,
            &auth(),
        )
        .await
        .unwrap_or_else(|err| panic!("curseforge browse {text:?}: {err}"))
}

#[tokio::test]
#[ignore = "hits api.curseforge.com"]
async fn curseforge_features_forever_addons() {
    if !has_curseforge_key() {
        return;
    }

    let results = curseforge_browse("").await;

    assert!(
        results.addons.len() >= 10,
        "expected the featured lists to name a dozen or more addons, got {}",
        results.addons.len()
    );
    assert!(
        results
            .addons
            .iter()
            .all(|addon| addon.expansions.contains(&Expansion::Forever)),
        "every featured row is a Forever addon"
    );
    assert!(results.addons.iter().all(|addon| !addon.name.is_empty()));
}

/// Passes whether or not the key may search: it checks that a search either
/// answers, or falls back to the featured list with a notice.
#[tokio::test]
#[ignore = "hits api.curseforge.com"]
async fn curseforge_searches_or_says_why_it_cannot() {
    if !has_curseforge_key() {
        return;
    }

    let results = curseforge_browse("boss").await;
    println!("search access: {:?}", results.access);

    match results.access {
        SearchAccess::Allowed => {
            assert!(
                !results.addons.is_empty(),
                "a search for boss finds something"
            );
            assert!(results.notice.is_none());
        }
        SearchAccess::Forbidden => assert!(results.notice.is_some()),
        SearchAccess::Untested => panic!("a text query always tests search"),
    }
}

#[tokio::test]
#[ignore = "hits api.curseforge.com"]
async fn curseforge_finds_an_addon_by_project_id_and_resolves_its_download() {
    if !has_curseforge_key() {
        return;
    }

    let featured = curseforge_browse("").await.addons;
    let wanted = featured
        .iter()
        .find(|addon| addon.is_installable())
        .expect("a featured addon allows API distribution");

    let by_id = curseforge_browse(wanted.id.key.as_str()).await;
    assert_eq!(by_id.addons.len(), 1);
    assert_eq!(by_id.addons[0].id, wanted.id);

    let summary = sources()
        .lookup_live(LiveSource::CurseForge, &wanted.id.key, &auth())
        .await
        .unwrap_or_else(|err| panic!("curseforge lookup: {err}"));
    let detail = sources()
        .fetch_live_detail(LiveSource::CurseForge, &wanted.id.key, &auth())
        .await
        .unwrap_or_else(|err| panic!("curseforge detail: {err}"));
    assert_eq!(detail.summary.id, summary.id);
    assert!(!detail.description.is_empty());

    let url = sources()
        .resolve_archive_url(&summary, &auth())
        .await
        .unwrap_or_else(|err| panic!("curseforge download: {err}"));
    assert!(
        url.starts_with("https://"),
        "unexpected download URL: {url}"
    );

    let many = sources()
        .lookup_live_many(
            LiveSource::CurseForge,
            std::slice::from_ref(&wanted.id.key),
            &auth(),
        )
        .await
        .unwrap_or_else(|err| panic!("curseforge bulk lookup: {err}"));
    assert_eq!(many.len(), 1);
}

#[tokio::test]
#[ignore = "hits addons.wago.io"]
async fn wago_publishes_a_forever_catalog() {
    if auth().wago_token.is_none() {
        println!("WAGO_TOKEN is not set; skipping the Wago live test");
        return;
    }

    let addons = catalog_of(CatalogSource::Wago).await;

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
    let addons = catalog_of(CatalogSource::GitHub).await;

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
