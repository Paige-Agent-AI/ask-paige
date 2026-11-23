//! Start the official, pinned GitHub MCP server without inheriting credentials.

use std::{io::Read, os::windows::process::CommandExt, process::Command};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::connected_tools::Service;

const EXECUTABLE_SHA256: &str = "38c58845e150f609d4fcd122bb8ef3c6ee5f8d5d869d69c00267e1638cadae9c";

pub(crate) fn command(app: &AppHandle) -> Result<Command, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|_| "GitHub MCP resources are unavailable.")?;
    let executable = resource_dir.join("providers").join("github-mcp-server.exe");
    let mut file = std::fs::File::open(&executable)
        .map_err(|_| "Install the PAIGE Windows build that includes GitHub MCP.")?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let length = file
            .read(&mut buffer)
            .map_err(|_| "Could not verify the GitHub MCP executable.")?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    if hex::encode(digest.finalize()) != EXECUTABLE_SHA256 {
        return Err("The bundled GitHub MCP executable failed its integrity check.".into());
    }
    drop(file);

    let tools = Service::GitHub
        .tools()
        .iter()
        .map(|tool| tool.name)
        .collect::<Vec<_>>()
        .join(",");
    let mut command = Command::new(executable);
    command
        .env_clear()
        .current_dir(resource_dir)
        .creation_flags(0x08000000);
    for name in [
        "SystemRoot",
        "WINDIR",
        "PATH",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
        "COMSPEC",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.args([
        "stdio",
        "--gh-host",
        "https://github.com",
        "--oauth-scopes",
        "repo",
        "--tools",
        &tools,
    ]);
    Ok(command)
}
