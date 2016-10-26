import os
import subprocess
import shutil


def run(*args):
    return subprocess.run(["cargo", "run"] + list(args), stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def assertbad(result):
    if result.returncode == 0:
        print(result.stdsout)
        print(result.stderr)


def test_should_require_a_command():
    assertbad(run())


def test_should_require_valid_command():
    assertbad(run("invalid"))


# this file should never be modified by our testing code
# instead we copy it into a test directory, and so on
SOURCE_FILE = "input.egsphsp1"
TEMP_DIR = "testing"


if __name__ == '__main__':
    source_files = [""]
    shutil.rmtree(TEMP_DIR, ignore_errors=True)
    os.makedirs(TEMP_DIR)
    shutil.copyfile(SOURCE_FILE, os.path.join(TEMP_DIR, SOURCE_FILE))
    test_should_require_a_command()
    test_should_require_valid_command()
