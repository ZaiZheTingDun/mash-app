"""Validate, summarize, and split digit dataset manifests."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from .schema import assign_grouped_splits, load_manifest, manifest_summary, save_manifest


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path, help="JSONL manifest to inspect")
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("validate", help="validate schema and unique IDs")
    subparsers.add_parser("stats", help="print dataset counts as JSON")

    split = subparsers.add_parser(
        "split", help="assign deterministic screenshot-grouped dataset splits"
    )
    split.add_argument("--seed", type=int, default=20260917)
    split.add_argument("--train", type=float, default=0.8)
    split.add_argument("--validation", type=float, default=0.1)
    split.add_argument("--test", type=float, default=0.1)
    return parser


def main() -> None:
    args = _parser().parse_args()
    samples = load_manifest(args.manifest)

    if args.command == "validate":
        print(f"valid: {len(samples)} samples")
        return
    if args.command == "stats":
        print(json.dumps(manifest_summary(samples), ensure_ascii=False, indent=2))
        return
    if args.command == "split":
        split_samples = assign_grouped_splits(
            samples,
            seed=args.seed,
            train_ratio=args.train,
            validation_ratio=args.validation,
            test_ratio=args.test,
        )
        save_manifest(args.manifest, split_samples)
        print(json.dumps(manifest_summary(split_samples), ensure_ascii=False, indent=2))
        return
    raise AssertionError(f"unhandled command: {args.command}")


if __name__ == "__main__":
    main()
