"""A JSON client that retains failed verdicts and distinguishes command failures."""
import argparse
import json
import subprocess
import sys


def invoke(binary, directory, args, env=None):
    process = subprocess.run([str(binary), "-C", str(directory), "--json", *args],
                             capture_output=True, text=True, env=env, check=False)
    try:
        data = json.loads(process.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"not a JSON response: {process.stdout!r}; {process.stderr}") from error
    if data.get("plied") != 1 or data.get("exit") != process.returncode:
        raise RuntimeError(f"unsupported envelope or exit mismatch: {data}")
    return process.returncode, data


def explanation(data):
    if "error" in data:
        error = data["error"]
        return {"message": error["message"], "detail": error.get("detail")}
    result = data.get("result")
    if isinstance(result, dict):
        if "diagnostics" in result:
            return result["diagnostics"]
        if result.get("gate") is not None:
            return result["gate"]["reasons"]
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plied", default="plied")
    parser.add_argument("-C", default=".", dest="directory")
    parser.add_argument("args", nargs=argparse.REMAINDER)
    options = parser.parse_args()
    args = options.args[1:] if options.args[:1] == ["--"] else options.args
    code, response = invoke(options.plied, options.directory, args)
    print(json.dumps({"exit": code, "explanation": explanation(response), "notes": response.get("notes", [])}, indent=2))
    sys.exit(code)
