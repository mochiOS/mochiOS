#!/usr/bin/env python3
"""Check UI framework dependency boundaries without building or running a GUI."""

from pathlib import Path
import re
import sys
import tomllib


def source_issues(project: Path) -> list[str]:
    """Reject Rust module imports that reach outside the owning project."""
    issues = []
    for source in (project / "src").rglob("*.rs"):
        text = source.read_text()
        targets = re.findall(r'#\[path\s*=\s*"([^"]+)"\]', text)
        targets += re.findall(r'include(?:_str|_bytes)?!\(\s*"([^"]+\.rs)"', text)
        for target in targets:
            if not (source.parent / target).resolve().is_relative_to(project.resolve()):
                issues.append(f"{source}: external Rust source import: {target}")
    return issues


def dependency_issues(project: Path, root: Path, *, viewkit: bool = False) -> list[str]:
    """Reject framework dependencies on application implementations or services."""
    issues = []

    def visit(table):
        for key, value in table.items():
            if not isinstance(value, dict):
                continue
            if key in ("dependencies", "dev-dependencies", "build-dependencies"):
                for name, dependency in value.items():
                    if viewkit and name == "mochios-appcore":
                        issues.append(f"{project}: ViewKit depends on AppCore")
                    if not isinstance(dependency, dict):
                        continue
                    package = dependency.get("package", name)
                    if viewkit and name != "mochios-appcore" and package == "mochios-appcore":
                        issues.append(f"{project}: ViewKit depends on AppCore")
                    if "path" in dependency:
                        target = (project / dependency["path"]).resolve()
                        if any(target.is_relative_to((root / area).resolve())
                               for area in ("applications", "services")):
                            issues.append(f"{project}: framework depends on {target}")
            else:
                visit(value)

    manifest = (project / "Cargo.toml").read_text()
    # Cargo accepts TOML 1.1 multiline inline tables; Python 3.12's tomllib
    # supports 1.0. Normalize only those tables for this read-only check.
    manifest = re.sub(
        r"\{[^{}]*\}",
        lambda match: re.sub(r",\s*\}$", " }", match[0].replace("\n", " ")),
        manifest,
    )
    visit(tomllib.loads(manifest))
    return issues


def audit(root: Path) -> list[str]:
    """Check framework direction, source isolation and mirrored elevation data."""
    issues = []
    for relative in ("libraries/viewkit", "tools/devkit/crates/appcore"):
        project = root / relative
        issues += dependency_issues(project, root, viewkit=relative == "libraries/viewkit")
        issues += source_issues(project)
    issues += source_issues(root / "services/compositor")
    elevation = "src/theme/window_elevation.rs"
    if ((root / "libraries/viewkit" / elevation).read_bytes()
            != (root / "services/compositor/src/window_elevation.rs").read_bytes()):
        issues.append("ViewKit/compositor window elevation copies differ")
    return issues


if __name__ == "__main__":
    failures = audit(Path(__file__).resolve().parents[2])
    if failures:
        print("\n".join(failures), file=sys.stderr)
        sys.exit(1)
    print("UI dependency boundaries and window elevation: OK")
