"""The `foliq` command. The whole CLI is written in Rust (crate `foliq-cli`)."""

import sys

from foliq._core import run_cli


def main() -> None:
    sys.exit(run_cli(sys.argv))


if __name__ == "__main__":
    main()
