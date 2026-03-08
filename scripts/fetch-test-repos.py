#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "requests",
# ]
# ///
"""Fetch popular open-source repos from package registries for testing mdlr.

Queries crates.io and npm to find the most downloaded packages,
resolves their GitHub repos, and clones them into test-repos/.
"""

import argparse
import subprocess
import sys
import time
from pathlib import Path

import requests

SCRIPT_DIR = Path(__file__).resolve().parent
REPOS_DIR = SCRIPT_DIR.parent / "test-repos"

CRATES_IO_API = "https://crates.io/api/v1/crates"
NPM_API = "https://registry.npmjs.org/-/v1/search"
GITHUB_API = "https://api.github.com/repos"

# Minimum repo size in KB to filter out trivially small packages
MIN_REPO_SIZE_KB = 500

# crates.io requires a user-agent
HEADERS_CRATES = {"User-Agent": "mdlr-test-repo-fetcher (https://github.com/mdlr)"}


def fetch_top_rust_repos(count: int) -> list[dict]:
    """Fetch top Rust crates by download count from crates.io."""
    repos = []
    page = 1
    per_page = min(count * 2, 100)  # fetch extra since some may lack a repo

    while len(repos) < count:
        resp = requests.get(
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
                # Normalize URL: strip .git suffix, trailing slashes
                repo_url = repo_url.rstrip("/")
                if repo_url.endswith(".git"):
                    repo_url = repo_url[:-4]
                # Deduplicate by repo URL (monorepos publish multiple crates)
                if not any(r["url"] == repo_url for r in repos):
                    repos.append({"name": crate["name"], "url": repo_url})
                    if len(repos) >= count:
                        break

        page += 1
        time.sleep(1)  # respect crates.io rate limit

    return repos


def github_repo_size_kb(repo_url: str) -> int | None:
    """Query GitHub API for repo size in KB. Returns None on failure."""
    # Extract owner/repo from URL like https://github.com/owner/repo
    parts = repo_url.rstrip("/").split("github.com/")[-1].split("/")
    if len(parts) < 2:
        return None
    owner_repo = f"{parts[0]}/{parts[1]}"
    try:
        resp = requests.get(f"{GITHUB_API}/{owner_repo}", headers=HEADERS_CRATES)
        time.sleep(1)  # respect GitHub unauthenticated rate limit (60 req/hour)
        if resp.status_code == 200:
            return resp.json().get("size", 0)
    except requests.RequestException:
        pass
    return None


def fetch_top_typescript_repos(count: int) -> list[dict]:
    """Fetch top TypeScript packages by popularity from npm."""
    repos = []
    offset = 0
    size = min(count * 3, 250)  # fetch extra: many npm packages share a repo

    while len(repos) < count:
        resp = requests.get(
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

            # Only GitHub repos
            if "github.com" not in repo_url:
                continue

            # Normalize
            repo_url = repo_url.replace("git+", "").replace("git://", "https://")
            repo_url = repo_url.rstrip("/")
            if repo_url.endswith(".git"):
                repo_url = repo_url[:-4]

            # Deduplicate by repo URL (monorepos publish many packages)
            if any(r["url"] == repo_url for r in repos):
                continue

            # Filter out small repos
            size_kb = github_repo_size_kb(repo_url)
            if size_kb is not None and size_kb < MIN_REPO_SIZE_KB:
                print(f"  skip {pkg['name']} ({size_kb} KB < {MIN_REPO_SIZE_KB} KB)")
                continue

            repos.append({"name": pkg["name"], "url": repo_url})
            if len(repos) >= count:
                break

        offset += size

    return repos


FETCHERS = {
    "rust": fetch_top_rust_repos,
    "typescript": fetch_top_typescript_repos,
}


def clone_repo(name: str, url: str, dest: Path, shallow: bool) -> None:
    if dest.exists():
        print(f"  skip {name} (already exists)")
        return

    print(f"  clone {name} <- {url}")
    cmd = ["git", "clone", "--quiet"]
    if shallow:
        cmd += ["--depth", "1"]
    cmd += [url, str(dest)]
    subprocess.run(cmd, check=True)


def main():
    parser = argparse.ArgumentParser(
        description="Fetch popular repos from package registries for testing mdlr."
    )
    parser.add_argument(
        "--lang",
        choices=["rust", "typescript", "all"],
        default="all",
        help="Language to fetch repos for (default: all)",
    )
    parser.add_argument(
        "--count",
        type=int,
        default=5,
        help="Number of repos per language (default: 10)",
    )
    parser.add_argument(
        "--shallow",
        action="store_true",
        help="Shallow clone (depth=1) to save disk space",
    )
    args = parser.parse_args()

    langs = list(FETCHERS.keys()) if args.lang == "all" else [args.lang]

    for lang in langs:
        print(f"[{lang}] fetching top {args.count} repos from registry...")
        repos = FETCHERS[lang](args.count)

        dest_dir = REPOS_DIR / lang
        dest_dir.mkdir(parents=True, exist_ok=True)

        print(f"[{lang}] cloning {len(repos)} repos:")
        for repo in repos:
            # Use repo name from URL (last path segment) for the directory
            dir_name = repo["url"].rstrip("/").split("/")[-1]
            clone_repo(repo["name"], repo["url"], dest_dir / dir_name, args.shallow)
        print()

    print(f"Done. Repos in {REPOS_DIR}")


if __name__ == "__main__":
    main()
