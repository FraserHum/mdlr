#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "httpx",
#     "typer",
# ]
# ///
"""Fetch popular open-source repos for testing mdlr.

Two modes:
  fetch   - Clone the curated top-5 repos per language (default).
  search  - Query package registries to discover top repos by popularity,
            then rank by source lines of code. Useful for refreshing the
            curated list.
"""

import subprocess
import time
from pathlib import Path

import httpx
import typer

app = typer.Typer(help="Fetch test repos for mdlr.")

SCRIPT_DIR = Path(__file__).resolve().parent
REPOS_DIR = SCRIPT_DIR.parent / "test-repos"

CRATES_IO_API = "https://crates.io/api/v1/crates"
NPM_API = "https://registry.npmjs.org/-/v1/search"
GITHUB_API = "https://api.github.com/repos"

MIN_REPO_SIZE_KB = 500

HEADERS_CRATES = {"User-Agent": "mdlr-test-repo-fetcher (https://github.com/mdlr)"}

# ── Curated repos (top 5 by source lines per language) ──────────────────

CURATED: dict[str, list[dict]] = {
    "rust": [
        {"name": "libc", "url": "https://github.com/rust-lang/libc"},
        {"name": "regex", "url": "https://github.com/rust-lang/regex"},
        {"name": "syn", "url": "https://github.com/dtolnay/syn"},
        {"name": "memchr", "url": "https://github.com/BurntSushi/memchr"},
        {"name": "serde", "url": "https://github.com/serde-rs/serde"},
    ],
    "typescript": [
        {"name": "typescript-eslint", "url": "https://github.com/typescript-eslint/typescript-eslint"},
        {"name": "babel", "url": "https://github.com/babel/babel"},
        {"name": "jest", "url": "https://github.com/jestjs/jest"},
        {"name": "zod", "url": "https://github.com/colinhacks/zod"},
        {"name": "typebox", "url": "https://github.com/sinclairzx81/typebox-legacy"},
    ],
}


# ── Registry fetchers (used by `search`) ────────────────────────────────


def _fetch_top_rust_repos(count: int) -> list[dict]:
    """Fetch top Rust crates by download count from crates.io."""
    repos: list[dict] = []
    page = 1
    per_page = min(count * 2, 100)

    while len(repos) < count:
        resp = httpx.get(
            CRATES_IO_API,
            params={"page": page, "per_page": per_page, "sort": "downloads"},
            headers=HEADERS_CRATES,
        )
        resp.raise_for_status()
        crates = resp.json()["crates"]
        if not crates:
            break

        for crate in crates:
            repo_url = crate.get("repository")
            if repo_url and "github.com" in repo_url:
                repo_url = repo_url.rstrip("/")
                if repo_url.endswith(".git"):
                    repo_url = repo_url[:-4]
                if not any(r["url"] == repo_url for r in repos):
                    repos.append({"name": crate["name"], "url": repo_url})
                    if len(repos) >= count:
                        break

        page += 1
        time.sleep(1)

    return repos


def _github_repo_size_kb(repo_url: str) -> int | None:
    """Query GitHub API for repo size in KB."""
    parts = repo_url.rstrip("/").split("github.com/")[-1].split("/")
    if len(parts) < 2:
        return None
    owner_repo = f"{parts[0]}/{parts[1]}"
    try:
        resp = httpx.get(f"{GITHUB_API}/{owner_repo}", headers=HEADERS_CRATES)
        time.sleep(1)
        if resp.status_code == 200:
            return resp.json().get("size", 0)
    except httpx.HTTPError:
        pass
    return None


def _fetch_top_typescript_repos(count: int) -> list[dict]:
    """Fetch top TypeScript packages by popularity from npm."""
    repos: list[dict] = []
    offset = 0
    size = min(count * 3, 250)

    while len(repos) < count:
        resp = httpx.get(
            NPM_API,
            params={
                "text": "keywords:typescript",
                "popularity": "1.0",
                "quality": "0.0",
                "maintenance": "0.0",
                "size": size,
                "from": offset,
            },
        )
        resp.raise_for_status()
        results = resp.json()["objects"]
        if not results:
            break

        for obj in results:
            pkg = obj["package"]
            links = pkg.get("links", {})
            repo_url = links.get("repository") or ""

            if "github.com" not in repo_url:
                continue

            repo_url = repo_url.replace("git+", "").replace("git://", "https://")
            repo_url = repo_url.rstrip("/")
            if repo_url.endswith(".git"):
                repo_url = repo_url[:-4]

            if any(r["url"] == repo_url for r in repos):
                continue

            size_kb = _github_repo_size_kb(repo_url)
            if size_kb is not None and size_kb < MIN_REPO_SIZE_KB:
                print(f"  skip {pkg['name']} ({size_kb} KB < {MIN_REPO_SIZE_KB} KB)")
                continue

            repos.append({"name": pkg["name"], "url": repo_url})
            if len(repos) >= count:
                break

        offset += size

    return repos


REGISTRY_FETCHERS = {
    "rust": _fetch_top_rust_repos,
    "typescript": _fetch_top_typescript_repos,
}

CLOC_LANG_KEY = {
    "rust": "Rust",
    "typescript": "TypeScript",
}


# ── Helpers ──────────────────────────────────────────────────────────────


def _clone_repo(name: str, url: str, dest: Path) -> None:
    if dest.exists():
        print(f"  skip {name} (already exists)")
        return
    print(f"  clone {name} <- {url}")
    cmd = ["git", "clone", "--quiet", "--depth", "1", url, str(dest)]
    subprocess.run(cmd, check=True)


def _cloc_lines(path: Path, lang_key: str) -> int:
    """Return source lines of `lang_key` in `path` using cloc --json."""
    import json

    try:
        result = subprocess.run(
            ["cloc", str(path), "--json"],
            capture_output=True,
            text=True,
            check=True,
        )
        data = json.loads(result.stdout)
        return data.get(lang_key, {}).get("code", 0)
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        return 0


# ── Commands ─────────────────────────────────────────────────────────────


@app.command()
def fetch(
    lang: str = typer.Option("all", help="Language to fetch (rust, typescript, all)."),
):
    """Clone the curated top-5 repos per language."""
    langs = list(CURATED.keys()) if lang == "all" else [lang]

    for lg in langs:
        repos = CURATED[lg]
        dest_dir = REPOS_DIR / lg
        dest_dir.mkdir(parents=True, exist_ok=True)

        print(f"[{lg}] cloning {len(repos)} curated repos:")
        for repo in repos:
            dir_name = repo["url"].rstrip("/").split("/")[-1]
            _clone_repo(repo["name"], repo["url"], dest_dir / dir_name)
        print()

    print(f"Done. Repos in {REPOS_DIR}")


@app.command()
def search(
    lang: str = typer.Option("all", help="Language to search (rust, typescript, all)."),
    count: int = typer.Option(20, help="Number of repos to fetch from registries."),
    top: int = typer.Option(5, help="Number of top repos to keep after ranking by LOC."),
):
    """Discover top repos by popularity, clone them, rank by source LOC."""
    langs = list(REGISTRY_FETCHERS.keys()) if lang == "all" else [lang]

    for lg in langs:
        cloc_key = CLOC_LANG_KEY[lg]

        print(f"[{lg}] fetching top {count} repos from registry...")
        repos = REGISTRY_FETCHERS[lg](count)

        dest_dir = REPOS_DIR / lg
        dest_dir.mkdir(parents=True, exist_ok=True)

        print(f"[{lg}] cloning {len(repos)} repos:")
        for repo in repos:
            dir_name = repo["url"].rstrip("/").split("/")[-1]
            repo["dir"] = dest_dir / dir_name
            _clone_repo(repo["name"], repo["url"], repo["dir"])

        print(f"\n[{lg}] counting {cloc_key} lines of code...")
        for repo in repos:
            if "dir" not in repo:
                dir_name = repo["url"].rstrip("/").split("/")[-1]
                repo["dir"] = dest_dir / dir_name
            repo["loc"] = _cloc_lines(repo["dir"], cloc_key)

        repos.sort(key=lambda r: r["loc"], reverse=True)

        print(f"\n[{lg}] top {top} repos by {cloc_key} LOC:")
        for i, repo in enumerate(repos[:top], 1):
            print(f"  {i}. {repo['name']:30s} {repo['loc']:>8,} lines  {repo['url']}")

        # Remove repos outside the top N
        keep_dirs = {r["dir"] for r in repos[:top]}
        for repo in repos[top:]:
            d = repo.get("dir")
            if d and d.exists():
                print(f"  remove {repo['name']} ({repo['loc']:,} lines)")
                subprocess.run(["rm", "-rf", str(d)], check=True)

        print()

    print(f"Done. Repos in {REPOS_DIR}")


if __name__ == "__main__":
    app()
