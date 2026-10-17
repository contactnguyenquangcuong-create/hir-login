// Write the MCP server source into a user-chosen folder.  The app does
// NOT run or manage it — the user installs deps + registers it with
// their MCP client themselves (see mcp/README.md).
//
// The three files of the repo's own `mcp/` folder are compiled into the
// binary, so the button needs no network and always hands out the server
// that matches this build of the app.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// `mcp/` at the repo root, embedded at build time.
const MCP_FILES: &[(&str, &[u8])] = &[
    ("index.js", include_bytes!("../../mcp/index.js")),
    ("package.json", include_bytes!("../../mcp/package.json")),
    ("README.md", include_bytes!("../../mcp/README.md")),
];

/// Write the MCP server into `<dir>/mcp` and return that path.  Files
/// already there are overwritten (so re-downloading updates the server);
/// anything else in the folder, such as `node_modules`, is left alone.
pub async fn download_mcp(dir: &Path) -> Result<PathBuf> {
    let dest = dir.join("mcp");
    std::fs::create_dir_all(&dest).with_context(|| format!("create {}", dest.display()))?;
    for (name, bytes) in MCP_FILES {
        let out = dest.join(name);
        std::fs::write(&out, bytes).with_context(|| format!("write {}", out.display()))?;
    }
    Ok(dest)
}
