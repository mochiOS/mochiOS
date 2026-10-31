#!/usr/bin/env python3
"""Copy a bundle's declared resources for both native staging and Linux preview."""
from pathlib import Path, PurePosixPath
import shutil
import sys
import tomllib


def stage(source: Path, destination: Path) -> None:
    """Copy declared, bundle-local files into their installed relative locations."""
    source = source.resolve()
    application = tomllib.loads((source / "manifest.toml").read_text()).get("application", {})
    for resource in application.get("resources", []):
        if not isinstance(resource, str) or not resource or resource.startswith("/") or ".." in PurePosixPath(resource).parts:
            raise ValueError(f"Invalid application resource: {resource!r}")
        candidates = [source / resource, source / "resources" / resource]
        origin = next((path for path in candidates if path.is_file()), None)
        if origin is None or not origin.resolve().is_relative_to(source):
            raise ValueError(f"Missing or external application resource: {resource}")
        target = destination / resource
        if not target.parent.resolve().is_relative_to(destination.resolve()):
            raise ValueError(f"External destination directory: {resource}")
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.is_symlink():
            target.unlink()
        shutil.copyfile(origin, target)


if __name__ == "__main__":
    stage(Path(sys.argv[1]), Path(sys.argv[2]))
