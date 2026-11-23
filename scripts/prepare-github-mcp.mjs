import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const directory = fileURLToPath(new URL("../src-tauri/target/provider-deps/", import.meta.url));
const archive = path.join(directory, "github-mcp-server-v2.2.0-windows-x64.zip");
const executable = path.join(directory, "github-mcp-server.exe");
const release = "https://github.com/github/github-mcp-server/releases/download/v2.2.0/github-mcp-server_Windows_x86_64.zip";
const archiveDigest = "6ecda644e8ea54b26acbc76e80379f4afb6ac1a4c2521366584f2a7f23e3a850";
const exeDigest = "38c58845e150f609d4fcd122bb8ef3c6ee5f8d5d869d69c00267e1638cadae9c";

function digest(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

mkdirSync(directory, { recursive: true });
if (!existsSync(archive)) {
  execFileSync("curl.exe", ["--fail", "--silent", "--show-error", "--location", "--max-time", "90", release, "--output", archive], { stdio: "inherit" });
}
if (digest(archive) !== archiveDigest) {
  throw new Error("The GitHub MCP archive differs from GitHub’s published SHA-256.");
}
execFileSync("tar.exe", ["-xf", archive, "-C", directory, "github-mcp-server.exe", "LICENSE"], { stdio: "inherit" });
if (digest(executable) !== exeDigest) {
  throw new Error("The GitHub MCP executable failed its SHA-256 check.");
}
console.log("Prepared verified GitHub MCP Server 2.2.0 for Windows x64.");
