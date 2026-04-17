#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Read/debug NDJSON log safely.")
    parser.add_argument(
        "--log",
        default=".cursor/debug-bf0ab3.log",
        help="Path to debug log file (default: .cursor/debug-bf0ab3.log)",
    )
    parser.add_argument(
        "--messages",
        nargs="*",
        default=[],
        help="Optional message names to include (space-separated).",
    )
    parser.add_argument(
        "--show-invalid",
        action="store_true",
        help="Print invalid line numbers as they are skipped.",
    )
    args = parser.parse_args()

    path = Path(args.log)
    if not path.exists():
        print(f"log file not found: {path}")
        return 1

    wanted = set(args.messages)
    valid = 0
    invalid = 0
    matched = 0

    with path.open("r", encoding="utf-8", errors="ignore") as fh:
        for line_no, raw in enumerate(fh, 1):
            line = raw.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except Exception:
                invalid += 1
                if args.show_invalid:
                    print(f"{line_no}: [invalid-json]")
                continue

            valid += 1
            msg = obj.get("message")
            if wanted and msg not in wanted:
                continue

            matched += 1
            location = obj.get("location")
            data = obj.get("data")
            print(f"{line_no}: {msg} | {location} | {data}")

    print(
        f"\nsummary: valid={valid} invalid={invalid} matched={matched} "
        f"filter={'none' if not wanted else ','.join(sorted(wanted))}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
