#!/usr/bin/env python3
"""Preview/create missing managed GitHub labels; never delete or overwrite."""
import argparse
import json
from pathlib import Path
import subprocess


def gh(*args):
    return subprocess.check_output(["gh", *args], text=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="fission-systems/Fission")
    parser.add_argument("--apply", action="store_true", help="create missing labels")
    args = parser.parse_args()
    catalog = json.loads(
        (Path(__file__).resolve().parents[2] / ".github/labels.json").read_text()
    )
    pages = json.loads(gh("api", "--paginate", "--slurp",
                         f"repos/{args.repo}/labels?per_page=100"))
    existing = {item["name"].casefold(): item for page in pages for item in page}
    for item in catalog:
        name = item["name"]
        previous = existing.get(name.casefold())
        if previous is not None:
            if (previous["color"].lower() != item["color"]
                    or (previous.get("description") or "") != item["description"]):
                print(f"DRIFT (preserved): {name}")
            continue
        print(f"{'CREATE' if args.apply else 'WOULD CREATE'}: {name}")
        if args.apply:
            subprocess.run(
                ["gh", "label", "create", name, "--repo", args.repo,
                 "--color", item["color"], "--description", item["description"]],
                check=True,
            )


if __name__ == "__main__":
    main()
