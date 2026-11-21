#!/usr/bin/env python3
"""Record a conveyor governance act, read the row back, and refuse unverified claims.

One command performs a write through the deployed engine, reads the affected row (or list)
back, and exits non-zero when the read-back fails.  A write is any action that is not
read-only.  A write requires --evidence PATH (which must exist before anything is invoked)
and a resolvable read-back action; when either is missing nothing is invoked.

Transport (copied from scripts/ops-live-proof.sh):

  ssh -i "$CONVEYOR_SSH_KEY" -o ConnectTimeout=10 "$CONVEYOR_HOST" \
    sudo KUBECONFIG=/etc/rancher/k3s/k3s.yaml kubectl -n "$CONVEYOR_NAMESPACE" \
    exec -i "$CONVEYOR_DEPLOY" -- conveyor admin action invoke \
    --tenant "$TENANT" --name ACTION --input /dev/stdin \
    [--approval-id UUID] [--actor-id UUID]

JSON goes in on stdin; pretty JSON comes out on stdout.

Environment (real transport):
  CONVEYOR_SSH_KEY, CONVEYOR_HOST   required
  CONVEYOR_NAMESPACE                default conveyor
  CONVEYOR_DEPLOY                   default deploy/conveyor
  TENANT                            default nonagentic

PROGRAMME_RECORD_CMD (or --command) replaces the transport with a stub command; --self-test
uses that hook and never touches the estate.

Read-back map (write -> read), used when --read-action is not given:
  tranches.authorise        -> tranches.list            (needs programme_id)
  programmes.baseline       -> programmes.baselines_list
  projects.programme_meta   -> projects.get
  architecture.board_decide -> architecture.board_get
  approvals.decide          -> approvals.list
  documents.publish         -> documents.get
  pm.close                  -> pm.get
  projects.reopen           -> projects.get
  projects.charter          -> pm.get

Python 3 stdlib only; no shell strings, no network libraries.
"""

import argparse
import json
import os
import shlex
import subprocess
import sys
import tempfile
import uuid

DEFAULT_NAMESPACE = "conveyor"
DEFAULT_DEPLOY = "deploy/conveyor"
DEFAULT_TENANT = "nonagentic"
DEFAULT_READ_ACTION = "programmes.list"

READBACK_MAP = {
    "tranches.authorise": "tranches.list",
    "tranches.update": "tranches.list",
    "programmes.baseline": "programmes.baselines_list",
    "programmes.baseline_approve": "programmes.baselines_list",
    "projects.programme_meta": "projects.get",
    "architecture.board_decide": "architecture.board_get",
    "approvals.decide": "approvals.list",
    "documents.publish": "documents.get",
    "documents.compose": "documents.get",
    "registers.create": "registers.get",
    "registers.update": "registers.get",
    "pm.close": "pm.get",
    "projects.close_request": "projects.get",
    "projects.reopen": "projects.get",
    "projects.charter": "pm.get",
}

# A read-back that needs identifiers different from the write's input.
READBACK_REQUIRED_INPUT = {"tranches.list": ("programme_id",)}

# Explicit allowlist; the suffix rule below keeps list/get actions read-only without
# having to enumerate every read action in the registry.
READ_ONLY_ACTIONS = {
    "tranches.list",
    "programmes.list",
    "programmes.get",
    "programmes.baselines_list",
    "projects.get",
    "architecture.board_get",
    "approvals.list",
    "approvals.get",
    "documents.get",
    "pm.get",
}


def fail(message, code=1):
    print(f"FAILED: {message}", file=sys.stderr)
    raise SystemExit(code)


def is_read_only(action):
    if action in READ_ONLY_ACTIONS:
        return True
    method = action.rsplit(".", 1)[-1]
    return method.endswith("list") or method.endswith("get")


def load_json_arg(raw, label):
    if raw.startswith("@"):
        path = raw[1:]
        try:
            with open(path, encoding="utf-8") as handle:
                text = handle.read()
        except OSError as error:
            fail(f"{label}: cannot read {path}: {error}", 2)
    else:
        text = raw
    try:
        value = json.loads(text)
    except ValueError as error:
        fail(f"{label}: not JSON: {error}", 2)
    if not isinstance(value, dict):
        fail(f"{label}: must be a JSON object", 2)
    return value


def uuid_arg(raw, label):
    if raw is None:
        return None
    try:
        uuid.UUID(raw)
    except ValueError:
        fail(f"{label}: not a UUID: {raw!r}", 2)
    return raw


def shell_join(command):
    return " ".join(shlex.quote(part) for part in command)


class Transport:
    """Builds the argv for the deployed CLI (or the self-test stub)."""

    def __init__(self, command=None):
        self.command = command or os.environ.get("PROGRAMME_RECORD_CMD")
        self.tenant = os.environ.get("TENANT") or DEFAULT_TENANT

    def build(self, action, approval_id=None, actor_id=None, if_match=None):
        tail = [
            "conveyor", "admin", "action", "invoke",
            "--tenant", self.tenant,
            "--name", action,
            "--input", "/dev/stdin",
        ]
        if approval_id:
            tail += ["--approval-id", approval_id]
        if actor_id:
            tail += ["--actor-id", actor_id]
        if if_match is not None:
            tail += ["--if-match", str(if_match)]
        if self.command:
            return shlex.split(self.command) + tail
        key = os.environ.get("CONVEYOR_SSH_KEY")
        host = os.environ.get("CONVEYOR_HOST")
        if not key:
            fail("CONVEYOR_SSH_KEY is required for the real transport", 2)
        if not host:
            fail("CONVEYOR_HOST is required for the real transport", 2)
        namespace = os.environ.get("CONVEYOR_NAMESPACE") or DEFAULT_NAMESPACE
        deploy = os.environ.get("CONVEYOR_DEPLOY") or DEFAULT_DEPLOY
        return [
            "ssh", "-i", key, "-o", "ConnectTimeout=10", host,
            "sudo", "KUBECONFIG=/etc/rancher/k3s/k3s.yaml",
            "kubectl", "-n", namespace, "exec", "-i", deploy, "--",
        ] + tail


def execute(transport, action, payload, approval_id=None, actor_id=None, if_match=None):
    command = transport.build(
        action, approval_id=approval_id, actor_id=actor_id, if_match=if_match
    )
    stdin = json.dumps(payload, separators=(",", ":")) + "\n"
    try:
        completed = subprocess.run(
            command,
            input=stdin,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
    except OSError as error:
        fail(f"{action}: cannot run transport: {error}")
    if completed.returncode != 0:
        if completed.stderr:
            sys.stderr.write(completed.stderr)
        if completed.stdout.strip():
            sys.stderr.write(completed.stdout)
        fail(f"{action}: transport exited {completed.returncode}")
    try:
        return json.loads(completed.stdout)
    except ValueError:
        fail(f"{action}: transport output is not JSON: {completed.stdout[:500]!r}")


def require_evidence(path):
    if not path:
        fail("a write requires --evidence PATH; the path must exist before the action runs")
    if not os.path.exists(path):
        fail(f"evidence path does not exist: {path}")


def resolve_readback(action, write_input, read_action_arg, read_input_arg):
    read_action = read_action_arg or READBACK_MAP.get(action)
    if not read_action:
        fail(f"{action}: no read-back mapping; pass --read-action (cannot verify the write)")
    if read_input_arg is not None:
        read_input = load_json_arg(read_input_arg, "--read-input")
    else:
        read_input = dict(write_input)
    missing = [key for key in READBACK_REQUIRED_INPUT.get(read_action, ()) if key not in read_input]
    if missing:
        fail(
            f"{action}: read-back {read_action} needs {', '.join(missing)}; "
            "pass --read-input JSON"
        )
    return read_action, read_input


def has_row(value):
    """A read-back row exists: non-null, non-empty, and not an empty list/get payload."""
    if value is None:
        return False
    if isinstance(value, bool):
        return value
    if isinstance(value, (str, list, dict)) and not value:
        return False
    if isinstance(value, dict):
        if value.get("ok") is False:
            return False
        for key in ("items", "rows", "results"):
            candidate = value.get(key)
            if isinstance(candidate, (list, dict)):
                return bool(candidate)
    return True


def print_result(kind, action, value):
    print(f"== {kind}: {action} ==")
    print(json.dumps(value, indent=2))


def print_plan(label, action, payload, command, evidence=None):
    print(f"== dry-run {label}: {action} ==")
    if evidence is not None:
        print(f"evidence: {evidence}")
    print(f"command: {shell_join(command)}")
    print(f"input: {json.dumps(payload, separators=(',', ':'))}")


def run_check(args, transport):
    action = args.action or DEFAULT_READ_ACTION
    if not is_read_only(action):
        fail(f"--check is read-only and refuses the write action {action}")
    payload = load_json_arg(args.input, "--input") if args.input else {}
    actor_id = uuid_arg(args.actor_id, "--actor-id")
    command = transport.build(action, actor_id=actor_id)
    if args.dry_run:
        print_plan("check", action, payload, command)
        return 0
    result = execute(transport, action, payload, actor_id=actor_id)
    print_result("check", action, result)
    return 0


def run_record(args, transport):
    action = args.action
    if action is None:
        fail("--action NAME is required (or use --check)", 2)
    if not args.input:
        fail("--input JSON|@FILE is required", 2)
    payload = load_json_arg(args.input, "--input")
    actor_id = uuid_arg(args.actor_id, "--actor-id")
    approval_id = uuid_arg(args.approval_id, "--approval-id")
    if_match = args.if_match

    if is_read_only(action):
        command = transport.build(action, actor_id=actor_id)
        if args.dry_run:
            print_plan("read", action, payload, command)
            return 0
        result = execute(transport, action, payload, actor_id=actor_id)
        print_result("read", action, result)
        return 0

    require_evidence(args.evidence)
    read_action, read_input = resolve_readback(
        action, payload, args.read_action, args.read_input
    )

    if args.dry_run:
        planned_read = (
            {"id": f"<from write output: {args.read_id_from_output}>"}
            if args.read_id_from_output
            else read_input
        )
        print_plan(
            "write", action, payload,
            transport.build(
                action, approval_id=approval_id, actor_id=actor_id, if_match=if_match
            ),
            evidence=args.evidence,
        )
        print_plan(
            "read-back", read_action, planned_read,
            transport.build(read_action, actor_id=actor_id),
        )
        return 0

    write_result = execute(
        transport, action, payload, approval_id=approval_id, actor_id=actor_id,
        if_match=if_match,
    )
    print_result("write", action, write_result)
    if args.read_id_from_output:
        created_id = write_result.get(args.read_id_from_output)
        if not isinstance(created_id, str) or not created_id:
            fail(
                f"the write returned no {args.read_id_from_output!r}; cannot read it back"
            )
        read_input = {"id": created_id}
    read_result = execute(transport, read_action, read_input, actor_id=actor_id)
    if not has_row(read_result):
        fail(f"read-back {read_action} returned no row; the {action} write is unverified")
    print_result("read-back", read_action, read_result)
    return 0


SELF_TEST_STUB = """#!/usr/bin/env python3
import json
import os
import sys

argv = sys.argv[1:]
raw = sys.stdin.read()
try:
    payload = json.loads(raw) if raw.strip() else {}
except ValueError:
    payload = None

log = os.environ.get("PROGRAMME_RECORD_LOG")
if log:
    with open(log, "a", encoding="utf-8") as handle:
        handle.write(json.dumps({"argv": argv, "input": payload}) + "\\n")

name = argv[argv.index("--name") + 1] if "--name" in argv else None
if name and os.environ.get("PROGRAMME_RECORD_EMPTY") == name:
    print("null")
elif name == "documents.compose":
    print(json.dumps({"id": "doc-1", "title": "composed"}))
else:
    print(json.dumps({"ok": True, "action": name, "input": payload}))
"""


def self_test():
    script = os.path.abspath(__file__)
    workdir = tempfile.mkdtemp(prefix="programme-record-selftest-")
    stub = os.path.join(workdir, "transport-stub.py")
    log = os.path.join(workdir, "calls.jsonl")
    evidence = os.path.join(workdir, "evidence.txt")
    with open(stub, "w", encoding="utf-8") as handle:
        handle.write(SELF_TEST_STUB)
    with open(evidence, "w", encoding="utf-8") as handle:
        handle.write("self-test evidence\n")

    cleared = (
        "CONVEYOR_SSH_KEY", "CONVEYOR_HOST", "CONVEYOR_NAMESPACE", "CONVEYOR_DEPLOY",
        "TENANT", "PROGRAMME_RECORD_CMD", "PROGRAMME_RECORD_LOG", "PROGRAMME_RECORD_EMPTY",
    )
    failures = []

    def child(argv, extra_env):
        env = {key: value for key, value in os.environ.items() if key not in cleared}
        env.update(extra_env)
        return subprocess.run(
            [sys.executable, "-I", script, *argv],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            env=env,
            cwd=workdir,
        )

    def calls():
        if not os.path.exists(log):
            return []
        with open(log, encoding="utf-8") as handle:
            return [json.loads(line) for line in handle if line.strip()]

    def actions():
        found = []
        for call in calls():
            argv = call["argv"]
            found.append(argv[argv.index("--name") + 1])
        return found

    def run_case(name, argv, extra_env, should_pass, verify=None):
        if os.path.exists(log):
            os.remove(log)
        completed = child(argv, extra_env)
        problems = []
        if should_pass and completed.returncode != 0:
            problems.append(f"exit {completed.returncode}, expected 0")
        if not should_pass and completed.returncode == 0:
            problems.append("exit 0, expected non-zero")
        if verify:
            try:
                problems.extend(verify(completed))
            except Exception as error:  # a broken assertion is a failure
                problems.append(f"verification raised {error!r}")
        if problems:
            failures.append(name)
            print(f"FAIL {name}")
            for problem in problems:
                print(f"  - {problem}")
            print(f"  stdout: {completed.stdout.strip()[:1500]}")
            print(f"  stderr: {completed.stderr.strip()[:1500]}")
        else:
            print(f"PASS {name}")

    fake_env = {
        "PROGRAMME_RECORD_CMD": shlex.join([sys.executable, "-I", stub]),
        "PROGRAMME_RECORD_LOG": log,
    }
    real_env = {"CONVEYOR_SSH_KEY": "/keys/none", "CONVEYOR_HOST": "ops@example.invalid"}
    write_input = '{"programme_id":"p1","note":"write"}'
    read_input = '{"programme_id":"p1","note":"read"}'
    write_argv = [
        "--action", "tranches.authorise",
        "--input", write_input,
        "--read-input", read_input,
        "--evidence", evidence,
    ]

    run_case(
        "evidence-missing refusal makes no call",
        ["--action", "tranches.authorise", "--input", write_input, "--read-input", read_input],
        fake_env,
        False,
        lambda completed: (
            [] if "evidence" in (completed.stdout + completed.stderr).lower()
            else ["refusal does not mention evidence"]
        ) + ([] if not os.path.exists(log) else ["the transport was called"]),
    )

    def verify_dry_run(completed):
        problems = []
        if os.path.exists(log):
            problems.append("the stub transport was called")
        for needle in (
            "command:",
            "evidence:",
            "--name tranches.authorise",
            "--name tranches.list",
            '"programme_id":"p1"',
            '"note":"write"',
            '"note":"read"',
        ):
            if needle not in completed.stdout:
                problems.append(f"dry-run output is missing {needle!r}")
        planned = child(write_argv + ["--dry-run"], real_env)
        if planned.returncode != 0:
            problems.append(f"real dry-run exited {planned.returncode}: {planned.stderr.strip()}")
        for needle in (
            "ssh -i /keys/none",
            "-o ConnectTimeout=10",
            "ops@example.invalid",
            "sudo KUBECONFIG=/etc/rancher/k3s/k3s.yaml",
            "kubectl -n conveyor exec -i deploy/conveyor --",
            "conveyor admin action invoke",
            "--tenant nonagentic",
            "--name tranches.authorise",
            "--input /dev/stdin",
        ):
            if needle not in planned.stdout:
                problems.append(f"real transport command is missing {needle!r}")
        return problems

    run_case(
        "dry-run prints and calls nothing",
        write_argv + ["--dry-run"],
        fake_env,
        True,
        verify_dry_run,
    )

    def verify_write(completed):
        problems = []
        if actions() != ["tranches.authorise", "tranches.list"]:
            problems.append(f"calls were {actions()!r}")
        else:
            recorded = calls()
            if recorded[0]["input"] != json.loads(write_input):
                problems.append("the write did not receive --input")
            if recorded[1]["input"] != json.loads(read_input):
                problems.append("the read-back did not receive --read-input")
        for needle in ("== write: tranches.authorise ==", "== read-back: tranches.list =="):
            if needle not in completed.stdout:
                problems.append(f"stdout is missing {needle!r}")
        return problems

    run_case(
        "write with read-back prints both",
        write_argv,
        fake_env,
        True,
        verify_write,
    )

    def verify_if_match(completed):
        problems = []
        recorded = calls()
        if len(recorded) != 2:
            problems.append(f"calls were {actions()!r}")
        else:
            write = recorded[0]["argv"]
            read = recorded[1]["argv"]
            if "--if-match" not in write:
                problems.append("the write did not carry --if-match")
            elif write[write.index("--if-match") + 1] != "7":
                problems.append("the write carried the wrong version")
            if "--if-match" in read:
                problems.append("the read-back carried --if-match")
        return problems

    run_case(
        "if-match is passed to the write only",
        write_argv + ["--if-match", "7"],
        fake_env,
        True,
        verify_if_match,
    )

    def verify_created_id(completed):
        problems = []
        if actions() != ["documents.compose", "documents.get"]:
            problems.append(f"calls were {actions()!r}")
        elif calls()[1]["input"] != {"id": "doc-1"}:
            problems.append(f"the read-back input was {calls()[1]['input']!r}")
        return problems

    run_case(
        "read-back id comes from the write output",
        [
            "--action", "documents.compose",
            "--input", '{"title":"t","body":"body"}',
            "--evidence", evidence,
            "--read-id-from-output", "id",
        ],
        fake_env,
        True,
        verify_created_id,
    )

    run_case(
        "write with no mapping refuses",
        ["--action", "ops.unmapped", "--input", "{}", "--evidence", evidence],
        fake_env,
        False,
        lambda completed: (
            [] if "read-back" in (completed.stdout + completed.stderr).lower()
            and "mapping" in (completed.stdout + completed.stderr).lower()
            else ["refusal does not explain the missing read-back"]
        ) + ([] if not os.path.exists(log) else ["the transport was called"]),
    )

    run_case(
        "check refuses a write action",
        ["--check", "--action", "tranches.authorise", "--input", '{"programme_id":"p1"}'],
        fake_env,
        False,
        lambda completed: (
            [] if "read-only" in (completed.stdout + completed.stderr).lower()
            else ["refusal does not say read-only"]
        ) + ([] if not os.path.exists(log) else ["the transport was called"]),
    )

    run_case(
        "check invokes a read action",
        ["--check", "--action", "tranches.list", "--input", '{"programme_id":"p1"}'],
        fake_env,
        True,
        lambda completed: (
            [] if actions() == ["tranches.list"] else [f"calls were {actions()!r}"]
        ) + (
            [] if "== check: tranches.list ==" in completed.stdout
            else ["stdout is missing the check header"]
        ),
    )

    run_case(
        "check uses the default read action",
        ["--check"],
        fake_env,
        True,
        lambda completed: (
            [] if actions() == [DEFAULT_READ_ACTION] else [f"calls were {actions()!r}"]
        ) + (
            [] if f"== check: {DEFAULT_READ_ACTION} ==" in completed.stdout
            else ["stdout is missing the check header"]
        ),
    )

    empty_env = dict(fake_env)
    empty_env["PROGRAMME_RECORD_EMPTY"] = "tranches.list"
    run_case(
        "read-back with no row fails",
        write_argv,
        empty_env,
        False,
        lambda completed: (
            [] if "== write: tranches.authorise ==" in completed.stdout
            else ["the write output was not printed"]
        ) + (
            [] if "unverified" in (completed.stdout + completed.stderr).lower()
            else ["failure does not say the claim is unverified"]
        ),
    )

    print()
    if failures:
        print(f"SELF-TEST FAILED: {len(failures)} of 10 cases: {', '.join(failures)}")
        return 1
    print("SELF-TEST OK: 10 cases passed")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Record a conveyor governance act and read it back.",
        epilog="Read-back map and transport are documented in the module docstring.",
    )
    parser.add_argument("--action", metavar="NAME", help="action to invoke")
    parser.add_argument("--input", metavar="JSON|@FILE", help="action input")
    parser.add_argument("--read-action", metavar="NAME", help="read-back action")
    parser.add_argument("--read-input", metavar="JSON|@FILE", help="read-back input")
    parser.add_argument("--evidence", metavar="PATH", help="required for a write")
    parser.add_argument("--approval-id", metavar="UUID", help="approved gate for the write")
    parser.add_argument("--actor-id", metavar="UUID", help="act as this actor")
    parser.add_argument(
        "--read-id-from-output",
        metavar="FIELD",
        help="read the row back by the id this field carries in the write output",
    )
    parser.add_argument(
        "--if-match", type=int, metavar="VERSION", help="the row version the write is made against"
    )
    parser.add_argument("--dry-run", action="store_true", help="print the plan; invoke nothing")
    parser.add_argument("--check", action="store_true", help="read-only probe")
    parser.add_argument("--self-test", action="store_true", help="run the fake-transport tests")
    parser.add_argument("--command", metavar="CMD", help="transport override (self-test hook)")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    transport = Transport(command=args.command)
    if args.check:
        return run_check(args, transport)
    return run_record(args, transport)


if __name__ == "__main__":
    sys.exit(main())
