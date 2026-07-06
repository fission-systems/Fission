import re

with open(".github/workflows/reusable-nir-check.yml", "r") as f:
    content = f.read()

# remove old Restore utils from cache
content = re.sub(r'      - name: Restore utils from cache\n        uses: actions/cache@v4\n        with:\n          path: ~/\.fission-utils\n          key: fission-utils-\$\{\{ inputs\.assets_tag \}\}\n\n', '', content)

if "Setup Utils" not in content:
    content = content.replace("          submodules: recursive\n", "          submodules: recursive\n\n      - name: Setup Utils\n        uses: ./.github/actions/setup-utils\n")

with open(".github/workflows/reusable-nir-check.yml", "w") as f:
    f.write(content)

