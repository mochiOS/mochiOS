import hashlib
import json
import os
import subprocess
import xml.etree.ElementTree as ET


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for data in iter(lambda: source.read(4 * 1024 * 1024), b""):
            value.update(data)
    return value.hexdigest()


def source_state(root):
    def git(directory, *args):
        return subprocess.check_output(["git", "-C", str(directory), *args])
    repositories = {}

    def visit(relative):
        path = root / relative
        if relative != "." and not (path / ".git").exists():
            repositories[relative] = {"initialized": False}
            return
        status = git(path, "status", "--porcelain").decode()
        changes = hashlib.sha256(git(path, "diff", "HEAD", "--binary"))
        for name in git(path, "ls-files", "--others", "--exclude-standard", "-z").split(b"\0"):
            if name:
                file = path / os.fsdecode(name)
                changes.update(name)
                if file.is_file() and not file.is_symlink():
                    changes.update(digest(file).encode())
        repositories[relative] = {"head": git(path, "rev-parse", "HEAD").decode().strip(),
                                  "dirty": bool(status), "changes_sha256": changes.hexdigest()}
        for row in git(path, "ls-files", "--stage", "-z").split(b"\0"):
            if row.startswith(b"160000 "):
                child = os.fsdecode(row.split(b"\t", 1)[1])
                visit(child if relative == "." else relative + "/" + child)
    visit(".")
    return repositories


def environment_state(root):
    tools = {}
    for name in ("rustc", "cargo", "mmake", "qemu-system-x86_64"):
        try:
            tools[name] = subprocess.check_output([name, "--version"], stderr=subprocess.STDOUT, timeout=10).decode(errors="replace").splitlines()[0]
        except (OSError, subprocess.SubprocessError) as error:
            tools[name] = {"unavailable": str(error)}
    files = {}
    for name in (".config", "out/newlib-port/sdk/lib/crt0.o", "out/newlib-port/sdk/lib/linker.ld",
                 "out/newlib-port/sdk/lib/libmochi_user_newlib_runtime.a", "out/newlib-port/sdk/sysroot/lib/libc.a"):
        path = root / name
        if path.is_file():
            files[name] = digest(path)
    return {"tools": tools, "input_sha256": files}


def save(directory, report, results):
    def xml_text(value):
        # Captured child stderr may contain binary/control bytes. Preserve the
        # original detail in JSON while keeping the JUnit document valid XML.
        return "".join(c if c in "\t\n\r" or 0x20 <= ord(c) <= 0xD7FF
                       or 0xE000 <= ord(c) <= 0xFFFD or 0x10000 <= ord(c) <= 0x10FFFF
                       else "\ufffd" for c in value)
    report["results"] = [r.record() for r in results]
    report["status"] = "pass" if results and all(r.status == "pass" for r in results) else "fail"
    (directory / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    suite = ET.Element("testsuite", name="mochios." + report["mode"], tests=str(len(results)),
                       failures=str(sum(r.status == "fail" for r in results)),
                       errors=str(sum(r.status == "error" for r in results)))
    for result in results:
        case = ET.SubElement(suite, "testcase", name=result.id, time=f"{result.seconds:.3f}")
        if result.status != "pass":
            ET.SubElement(case, "failure" if result.status == "fail" else "error").text = xml_text(result.detail)
        if result.log:
            ET.SubElement(case, "system-out").text = xml_text(result.log)
    ET.ElementTree(suite).write(directory / "junit.xml", encoding="utf-8", xml_declaration=True)
    temporary = directory.parent / (".latest-" + directory.name)
    temporary.symlink_to(directory.name, target_is_directory=True)
    temporary.replace(directory.parent / "latest")
