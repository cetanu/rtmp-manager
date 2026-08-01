use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use topcoat::asset::{Asset, Manifest, ManifestEntry, MANIFEST_NAME, MANIFEST_VERSION};

const TAILWIND_CSS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tailwind.css"));
// Vendored from topcoat-runtime 0.4.0 so the browser runtime is part of the
// same release artifact as the server executable.
const TOPCOAT_RUNTIME: &[u8] = include_bytes!("../static/topcoat-runtime.js");
const CHAT_EVENTS: &[u8] = include_bytes!("../static/chat-events.js");
// HLS.js 1.6.16 is distributed under Apache-2.0; see static/hls.LICENSE.txt.
const HLS_PLAYER: &[u8] = include_bytes!("../static/hls.min.js");
const HLS_LICENSE: &[u8] = include_bytes!("../static/hls.LICENSE.txt");
const STREAM_PREVIEW: &[u8] = include_bytes!("../static/stream-preview.js");

struct EmbeddedAsset {
    id: Asset,
    stem: &'static str,
    extension: &'static str,
    content_type: &'static str,
    contents: &'static [u8],
}

pub fn install(executable: &Path, tailwind_stylesheet: Asset) -> Result<()> {
    let executable_dir = executable
        .parent()
        .context("executable has no parent directory")?;
    let asset_dir = executable_dir.join("assets");
    fs::create_dir_all(&asset_dir)
        .with_context(|| format!("failed to create asset directory {}", asset_dir.display()))?;

    let assets = [
        EmbeddedAsset {
            id: tailwind_stylesheet,
            stem: "tailwind",
            extension: "css",
            content_type: "text/css",
            contents: TAILWIND_CSS,
        },
        EmbeddedAsset {
            id: topcoat::runtime::SCRIPT,
            stem: "topcoat",
            extension: "js",
            content_type: "text/javascript",
            contents: TOPCOAT_RUNTIME,
        },
        EmbeddedAsset {
            id: crate::web::CHAT_EVENTS_SCRIPT,
            stem: "chat-events",
            extension: "js",
            content_type: "text/javascript",
            contents: CHAT_EVENTS,
        },
        EmbeddedAsset {
            id: crate::web::HLS_PLAYER_SCRIPT,
            stem: "hls-player",
            extension: "js",
            content_type: "text/javascript",
            contents: HLS_PLAYER,
        },
        EmbeddedAsset {
            id: crate::web::HLS_PLAYER_LICENSE,
            stem: "hls-player-license",
            extension: "txt",
            content_type: "text/plain",
            contents: HLS_LICENSE,
        },
        EmbeddedAsset {
            id: crate::web::STREAM_PREVIEW_SCRIPT,
            stem: "stream-preview",
            extension: "js",
            content_type: "text/javascript",
            contents: STREAM_PREVIEW,
        },
    ];

    let entries = assets
        .iter()
        .map(|asset| write_asset(&asset_dir, asset))
        .collect::<Result<Vec<_>>>()?;
    Manifest {
        version: MANIFEST_VERSION,
        assets: entries,
    }
    .save(asset_dir.join(MANIFEST_NAME))
    .context("failed to write embedded asset manifest")?;

    Ok(())
}

fn write_asset(asset_dir: &Path, asset: &EmbeddedAsset) -> Result<ManifestEntry> {
    let hash = format!("{:x}", Sha256::digest(asset.contents));
    let file = format!("{}-{}.{}", asset.stem, &hash[..16], asset.extension);
    fs::write(asset_dir.join(&file), asset.contents)
        .with_context(|| format!("failed to write embedded asset {file}"))?;

    Ok(ManifestEntry {
        id: asset.id,
        file,
        hash,
        content_type: asset.content_type.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use topcoat::asset::AssetBundle;

    #[test]
    fn installs_a_loadable_bundle_with_every_runtime_asset() {
        let test_dir = std::env::temp_dir().join(format!(
            "rtmp-proxy-assets-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let executable = test_dir.join("rtmp-proxy");

        install(&executable, crate::web::TAILWIND_STYLESHEET).unwrap();
        let bundle = AssetBundle::load_dir(test_dir.join("assets")).unwrap();

        assert!(bundle.get(crate::web::TAILWIND_STYLESHEET).is_some());
        assert!(bundle.get(topcoat::runtime::SCRIPT).is_some());
        assert!(bundle.get(crate::web::CHAT_EVENTS_SCRIPT).is_some());
        assert!(bundle.get(crate::web::HLS_PLAYER_SCRIPT).is_some());
        assert!(bundle.get(crate::web::HLS_PLAYER_LICENSE).is_some());
        assert!(bundle.get(crate::web::STREAM_PREVIEW_SCRIPT).is_some());

        fs::remove_dir_all(test_dir).unwrap();
    }
}
