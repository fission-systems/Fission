#!/usr/bin/env python3
"""Collect GitHub release assets for local binary corpus testing.

This script intentionally keeps downloaded samples out of source control. It
emits a manifest with hashes and provenance, while the binary store should stay
under an ignored path such as benchmark/binary/realworld/.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_STORE = REPO_ROOT / "benchmark/binary/realworld/github"
DEFAULT_OUTPUT = REPO_ROOT / "benchmark/config/benchmark_corpus/github_release_samples.json"
DEFAULT_URL_LIST = REPO_ROOT / "benchmark/artifacts/corpus/github_release_asset_urls.txt"

DEFAULT_ASSET_INCLUDE = (
    r".*\.(exe|dll|sys|efi|elf|so|dylib|o|obj|bin|com|msi|zip|tar|gz|xz|7z)$"
)
DEFAULT_ASSET_EXCLUDE = r".*(source|src|debug|symbols?|pdb|dSYM|sha256|checksums?|asc|sig).*"

GITHUB_API = "https://api.github.com"


@dataclass(frozen=True)
class AssetPlan:
    repo: str
    tag: str
    name: str
    browser_download_url: str
    size: int | None
    content_type: str | None
    source_config_index: int


def request_json(url: str, timeout: float) -> Any:
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "fission-corpus-collector",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def github_api_url(path: str, params: dict[str, str] | None = None) -> str:
    url = f"{GITHUB_API}{path}"
    if params:
        url = f"{url}?{urllib.parse.urlencode(params)}"
    return url


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_sources(path: Path | None, repos: list[str]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    if path:
        payload = json.loads(path.read_text(encoding="utf-8"))
        raw_sources = payload.get("sources", payload if isinstance(payload, list) else [])
        if not isinstance(raw_sources, list):
            raise ValueError(f"source config must be a list or contain sources[]: {path}")
        for item in raw_sources:
            if not isinstance(item, dict):
                raise ValueError(f"source entry must be an object: {item!r}")
            rows.append(dict(item))
    for repo in repos:
        rows.append({"repo": repo})
    if not rows:
        raise ValueError("provide --source-config or at least one --repo")
    return rows


def compile_regex(pattern: str | None, default: str) -> re.Pattern[str]:
    return re.compile(pattern or default, re.IGNORECASE)


def normalize_repo(value: str) -> str:
    value = value.strip().strip("/")
    if value.startswith("https://github.com/"):
        value = value.removeprefix("https://github.com/").strip("/")
    parts = value.split("/")
    if len(parts) < 2:
        raise ValueError(f"repository must be owner/name: {value}")
    return f"{parts[0]}/{parts[1]}"


def release_objects(repo: str, source: dict[str, Any], timeout: float) -> list[dict[str, Any]]:
    tag = source.get("tag")
    if tag:
        return [request_json(github_api_url(f"/repos/{repo}/releases/tags/{tag}"), timeout)]
    if source.get("latest", True):
        return [request_json(github_api_url(f"/repos/{repo}/releases/latest"), timeout)]
    per_page = min(int(source.get("per_page", 30)), 100)
    releases = request_json(
        github_api_url(f"/repos/{repo}/releases", {"per_page": str(per_page)}),
        timeout,
    )
    if not isinstance(releases, list):
        raise ValueError(f"unexpected GitHub releases response for {repo}")
    return releases


def plan_assets(sources: list[dict[str, Any]], timeout: float) -> list[AssetPlan]:
    plans: list[AssetPlan] = []
    for index, source in enumerate(sources):
        repo = normalize_repo(str(source["repo"]))
        include = compile_regex(source.get("asset_include"), DEFAULT_ASSET_INCLUDE)
        exclude = compile_regex(source.get("asset_exclude"), DEFAULT_ASSET_EXCLUDE)
        max_assets = int(source.get("max_assets", 0) or 0)
        selected = 0
        try:
            releases = release_objects(repo, source, timeout)
        except urllib.error.HTTPError as exc:
            raise RuntimeError(f"failed to query {repo}: HTTP {exc.code}") from exc
        for release in releases:
            tag = str(release.get("tag_name") or release.get("name") or "untagged")
            for asset in release.get("assets") or []:
                name = str(asset.get("name") or "")
                url = str(asset.get("browser_download_url") or "")
                if not name or not url:
                    continue
                if not include.fullmatch(name) and not include.search(name):
                    continue
                if exclude.fullmatch(name) or exclude.search(name):
                    continue
                plans.append(
                    AssetPlan(
                        repo=repo,
                        tag=tag,
                        name=name,
                        browser_download_url=url,
                        size=asset.get("size"),
                        content_type=asset.get("content_type"),
                        source_config_index=index,
                    )
                )
                selected += 1
                if max_assets and selected >= max_assets:
                    break
            if max_assets and selected >= max_assets:
                break
    plans.sort(key=lambda item: (item.repo, item.tag, item.name))
    return plans


def safe_component(value: str) -> str:
    cleaned = "".join(ch if ch.isalnum() or ch in "._-" else "-" for ch in value)
    cleaned = "-".join(part for part in cleaned.split("-") if part)
    return cleaned.strip(".-_") or "unknown"


def download_asset(plan: AssetPlan, store: Path, timeout: float, force: bool) -> Path:
    repo_owner, repo_name = plan.repo.split("/", 1)
    target_dir = store / safe_component(repo_owner) / safe_component(repo_name) / safe_component(plan.tag)
    target_dir.mkdir(parents=True, exist_ok=True)
    target = target_dir / safe_component(plan.name)
    if target.exists() and not force:
        return target.resolve()
    headers = {"User-Agent": "fission-corpus-collector"}
    request = urllib.request.Request(plan.browser_download_url, headers=headers)
    with urllib.request.urlopen(request, timeout=timeout) as response:
        with tempfile.NamedTemporaryFile(dir=str(target_dir), delete=False) as tmp:
            shutil.copyfileobj(response, tmp)
            tmp_path = Path(tmp.name)
    tmp_path.replace(target)
    return target.resolve()


def repo_relative(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(REPO_ROOT))
    except ValueError:
        return str(path.resolve())


def make_manifest_entry(plan: AssetPlan, path: Path, digest: str, tags: list[str]) -> dict[str, Any]:
    repo_id = safe_component(plan.repo.replace("/", "-")).lower()
    asset_id = safe_component(Path(plan.name).stem).lower()
    entry_id = f"github-{repo_id}-{asset_id}-{digest[:12]}"
    return {
        "id": entry_id,
        "binary_path": repo_relative(path),
        "ghidra_project_key": entry_id,
        "role": "realworld_loader_smoke",
        "tags": sorted(set(["github_release", "realworld"] + tags)),
        "weight": 1,
        "metadata": {
            "sha256": digest,
            "size_bytes": path.stat().st_size,
            "github_repo": plan.repo,
            "github_release_tag": plan.tag,
            "github_asset_name": plan.name,
            "github_asset_url": plan.browser_download_url,
            "github_asset_size": plan.size,
            "github_asset_content_type": plan.content_type,
            "source_config_index": plan.source_config_index,
        },
    }


def write_url_list(plans: list[AssetPlan], output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        "\n".join(plan.browser_download_url for plan in plans) + ("\n" if plans else ""),
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-config", type=Path, help="JSON file with sources[].")
    parser.add_argument("--repo", action="append", default=[], help="GitHub repository owner/name. Repeatable.")
    parser.add_argument("--store", type=Path, default=DEFAULT_STORE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--url-list-output", type=Path, default=DEFAULT_URL_LIST)
    parser.add_argument("--download", action="store_true", help="Download selected assets and write manifest entries.")
    parser.add_argument("--dry-run", action="store_true", help="Only query releases and emit selected asset metadata.")
    parser.add_argument("--force", action="store_true", help="Re-download assets that already exist.")
    parser.add_argument("--timeout-sec", type=float, default=60.0)
    parser.add_argument("--tag", action="append", default=[], help="Extra manifest tag. Repeatable.")
    parser.add_argument("--name", default="github-release-samples")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    sources = load_sources(args.source_config, args.repo)
    plans = plan_assets(sources, args.timeout_sec)
    write_url_list(plans, args.url_list_output)
    if args.dry_run or not args.download:
        payload = {
            "asset_count": len(plans),
            "url_list_output": str(args.url_list_output),
            "assets": [plan.__dict__ for plan in plans],
        }
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0

    entries: list[dict[str, Any]] = []
    for plan in plans:
        path = download_asset(plan, args.store, args.timeout_sec, args.force)
        digest = sha256_file(path)
        entries.append(make_manifest_entry(plan, path, digest, list(args.tag)))
    payload = {
        "name": args.name,
        "suite_tier": "advisory",
        "gate_mode": "advisory",
        "notes": (
            "Generated by scripts/corpus/collect_github_release_samples.py. "
            "Downloaded binaries are local corpus artifacts and must not be committed."
        ),
        "entries": entries,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        json.dumps(
            {
                "asset_count": len(plans),
                "entry_count": len(entries),
                "output": str(args.output),
                "store": str(args.store),
                "url_list_output": str(args.url_list_output),
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BrokenPipeError:
        raise SystemExit(1)
