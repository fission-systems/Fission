import glob
import re

for filename in glob.glob(".github/workflows/*.yml"):
    with open(filename, "r") as f:
        content = f.read()

    # Remove the setup-sleigh or setup-utils jobs
    # They usually look like:
    #   setup-sleigh:
    #     uses: ./.github/workflows/reusable-setup-sleigh.yml
    #     with:
    #       assets_tag: ${{ inputs.assets_tag }}
    
    content = re.sub(r'  setup-(sleigh|utils):\n    uses: \.\/\.github\/workflows\/reusable-setup-(sleigh|utils)\.yml\n(?:    with:\n      assets_tag: \$\{\{ inputs\.assets_tag \}\}\n)?\n', '', content)

    # Remove needs: setup-sleigh
    content = re.sub(r'    needs: setup-(sleigh|utils)\n', '', content)

    # Change FISSION_SLEIGH_SPEC_DIR
    content = re.sub(r'\$\{\{ needs\.setup-(sleigh|utils)\.outputs\.sleigh_spec_dir \}\}', '${{ github.workspace }}/utils/sleigh-specs', content)

    with open(filename, "w") as f:
        f.write(content)
