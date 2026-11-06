"""What the agent is told in each setup.

All setups get the same instructions. `foliq_skill` also gets a list of
skills, written the way coding agents such as Claude Code show them: a name,
a one-line description and a file to read. Whether the agent reads the file
and uses foliq is part of what we measure.
"""

from pathlib import Path

SKILL_FILE = Path(__file__).resolve().parents[2] / "skills" / "foliq" / "SKILL.md"

# Where the skill file is put in the container, for the `foliq_skill` setup.
SKILL_PATH = "/skills/foliq/SKILL.md"

INSTRUCTIONS = """\
You are a research assistant with a Linux shell. The user's documents are in
the folder /docs: PDF papers and Excel workbooks. Answer the user's question
from these documents only, not from what you already know.

Give a short answer, then a "Sources:" list with the file and the page (for a
PDF) or the sheet (for a workbook) of each passage you used, for example:

Sources:
- /docs/papers/2201.00001.pdf, page 4
- /docs/spreadsheets/example.xlsx, sheet "Data"

If the documents do not have the answer, reply NOT FOUND and say in one
sentence what you looked for.
"""

SKILLS = """\

You have these skills. Before you use a skill, read its file.
- {name}: {description} (file: {path})
"""


def instructions(setup: str) -> str:
    """The system prompt for `setup`."""
    if setup != "foliq_skill":
        return INSTRUCTIONS
    name, description = skill_name_and_description()
    return INSTRUCTIONS + SKILLS.format(name=name, description=description, path=SKILL_PATH)


def skill_name_and_description() -> tuple[str, str]:
    """The `name` and `description` from the skill file's front matter."""
    fields = {}
    front_matter = SKILL_FILE.read_text().split("---")[1]
    for line in front_matter.strip().splitlines():
        key, _, value = line.partition(":")
        fields[key.strip()] = value.strip()
    return fields["name"], fields["description"]
