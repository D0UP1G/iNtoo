#!/usr/bin/env python3
"""Check lazy color imports without dependencies; optionally compare real outputs.

Use --python /path/to/venv/bin/python3 for installed color dependencies and
--benchmark for interleaved cold subprocess timings (no timing assertions).
All generated files and XDG directories live in a temporary directory.
"""
import argparse
import ast
import builtins
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest

ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "scripts/colors/generate_colors_material.py"
SOURCE = GENERATOR.read_text()
TREE = ast.parse(SOURCE)
IMAGE_IMPORTS = """    from PIL import Image
    from materialyoucolor.quantize import QuantizeCelebi
    from materialyoucolor.score.score import Score

"""
NUMPY_IMPORT = "    import numpy as np\n\n"


def eager_baseline():
    """Undo only this optimization, keeping every color algorithm identical."""
    assert SOURCE.count(IMAGE_IMPORTS) == 1
    assert SOURCE.count(NUMPY_IMPORT) == 1
    baseline = SOURCE.replace(IMAGE_IMPORTS, "").replace(NUMPY_IMPORT, "")
    baseline = baseline.replace("\ntry:\n    import tomllib", "\nimport numpy as np\n\ntry:\n    import tomllib", 1)
    return baseline.replace("from materialyoucolor.hct import Hct",
                            IMAGE_IMPORTS.replace("    ", "").rstrip() + "\nfrom materialyoucolor.hct import Hct", 1)


class ImportDemandTests(unittest.TestCase):
    def test_numpy_is_requested_only_when_auto_detection_runs(self):
        auto = next(node for node in TREE.body
                    if isinstance(node, ast.FunctionDef) and node.name == "_auto_detect_scheme")
        imports = []

        def importing(name, *args, **kwargs):
            imports.append(name)
            raise ModuleNotFoundError("blocked test dependency")

        namespace = {"__builtins__": {**vars(builtins), "__import__": importing}}
        exec(compile(ast.Module(body=[auto], type_ignores=[]), str(GENERATOR), "exec"), namespace)
        self.assertEqual(imports, [])
        with self.assertRaisesRegex(ModuleNotFoundError, "blocked test dependency"):
            namespace["_auto_detect_scheme"](object())
        self.assertEqual(imports, ["numpy"])

    def test_imports_are_owned_by_their_consumers(self):
        targets = {"numpy", "PIL", "materialyoucolor.quantize", "materialyoucolor.score.score"}
        top_level = {node.module if isinstance(node, ast.ImportFrom) else alias.name
                     for node in TREE.body if isinstance(node, (ast.Import, ast.ImportFrom))
                     for alias in node.names}
        self.assertFalse(targets & top_level)
        auto = next(node for node in TREE.body
                    if isinstance(node, ast.FunctionDef) and node.name == "_auto_detect_scheme")
        self.assertTrue(any(isinstance(node, ast.Import)
                            and any(alias.name == "numpy" for alias in node.names)
                            for node in ast.walk(auto)))

    def test_color_path_never_requests_image_or_numpy_imports(self):
        # Execute the actual source-selection branch with dependency-free stand-ins.
        branch = next(node for node in TREE.body if isinstance(node, ast.If)
                      and ast.unparse(node.test) == "args.path is not None")
        code = compile(ast.Module(body=[branch], type_ignores=[]), str(GENERATOR), "exec")
        imports = []
        image = SimpleNamespace(format="PNG", mode="RGB", size=(1, 1), getdata=lambda: [(128, 128, 128)])
        modules = {
            "PIL": SimpleNamespace(Image=SimpleNamespace(open=lambda path: image)),
            "materialyoucolor.quantize": SimpleNamespace(QuantizeCelebi=lambda pixels, count: {}),
            "materialyoucolor.score.score": SimpleNamespace(Score=SimpleNamespace(score=lambda colors: [123])),
        }

        def importing(name, *args, **kwargs):
            imports.append(name)
            if name not in modules:
                raise AssertionError(f"Unexpected import: {name}")
            return modules[name]

        for path, color, scheme, smart in [
            (None, "#1E88E5", "scheme-tonal-spot", False),
            ("fixture.png", None, "scheme-tonal-spot", False),
            ("fixture.png", "#1E88E5", "auto", False),
            ("fixture.png", None, "auto", True),
        ]:
            with self.subTest(path=path, color=color, scheme=scheme, smart=smart):
                imports.clear()
                auto_calls = []
                color_calls = []
                def auto_detect(value):
                    auto_calls.append(value)
                    return "scheme-monochrome"
                def from_color(value):
                    color_calls.append(value)
                    return 456
                args = SimpleNamespace(path=path, color=color, scheme=scheme,
                                       size=128, cache=None, smart=smart)
                namespace = {
                    "__builtins__": {**vars(builtins), "__import__": importing},
                    "args": args, "Hct": SimpleNamespace(from_int=lambda argb: SimpleNamespace(chroma=10)),
                    "hex_to_argb": from_color, "calculate_optimal_size": lambda w, h, size: (w, h),
                    "_auto_detect_scheme": auto_detect,
                }
                exec(code, namespace)
                self.assertEqual(imports, list(modules) if path else [])
                self.assertEqual(color_calls, [] if path else [color])
                self.assertEqual(len(auto_calls), int(path is not None and scheme == "auto"))
                self.assertEqual(args.scheme, "scheme-neutral" if smart else
                                 "scheme-monochrome" if scheme == "auto" else scheme)


def real_checks(python, benchmark, runs):
    available = subprocess.run([python, "-c", "import numpy, PIL, materialyoucolor"],
                               capture_output=True)
    if available.returncode:
        print(f"SKIP real output comparison and benchmark: color dependencies unavailable in {python}")
        return
    with tempfile.TemporaryDirectory(prefix="intoo-color-tests-") as temp:
        directory = Path(temp)
        baseline = directory / GENERATOR.name
        baseline.write_text(eager_baseline())
        env = dict(os.environ)
        for name in ("XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME", "HOME"):
            env[name] = str(directory / name.lower())
            Path(env[name]).mkdir()
        colorful = directory / "colorful.png"
        gray = directory / "gray.png"
        fixture = subprocess.run([python, "-c", """
from PIL import Image
import sys
image = Image.new('RGB', (24, 16))
image.putdata([((x * 31) % 256, (y * 47) % 256, ((x + y) * 67) % 256)
               for y in range(16) for x in range(24)])
image.save(sys.argv[1])
Image.new('RGB', (24, 16), (128, 128, 128)).save(sys.argv[2])
""", str(colorful), str(gray)], env=env, capture_output=True)
        assert fixture.returncode == 0, fixture.stderr.decode()
        output_options = ["json-output", "palette-output", "app-palette-output",
                          "terminal-output", "meta-output", "scss-output", "cache"]
        output_paths = {option: directory / (option + ".out") for option in output_options}
        outputs = [arg for option, path in output_paths.items() for arg in ("--" + option, str(path))]
        termscheme = str(ROOT / "scripts/colors/terminal/scheme-base.json")
        cases = [
            ("color dark", ["--color", "#1E88E5", "--mode", "dark", "--scheme", "scheme-tonal-spot", "--termscheme", termscheme]),
            ("color light", ["--color", "#1E88E5", "--mode", "light", "--scheme", "scheme-tonal-spot", "--termscheme", termscheme]),
            ("image explicit", ["--path", str(colorful), "--scheme", "scheme-tonal-spot"]),
            ("image auto", ["--path", str(colorful), "--scheme", "auto"]),
            ("gray auto smart", ["--path", str(gray), "--scheme", "auto", "--smart"]),
            ("gray explicit smart", ["--path", str(gray), "--scheme", "scheme-tonal-spot", "--smart"]),
            ("image wins over color", ["--path", str(colorful), "--color", "#1E88E5", "--scheme", "scheme-tonal-spot"]),
            ("help", ["--help"]),
            ("no input", []),
        ]

        def run(script, arguments):
            for path in output_paths.values():
                path.unlink(missing_ok=True)
            start = time.perf_counter()
            result = subprocess.run([python, str(script), *arguments], env=env,
                                    cwd=directory, capture_output=True)
            elapsed = time.perf_counter() - start
            files = {option: path.read_bytes() for option, path in output_paths.items() if path.exists()}
            return result, files, elapsed

        def stderr_without_locations(stderr, script):
            # Pillow warnings include the script path and line number, both of
            # which necessarily differ in the temporary eager reference.
            return re.sub(re.escape(os.fsencode(script)) + rb":\d+:",
                          b"GENERATOR:LINE:", stderr)

        for name, arguments in cases:
            arguments = arguments + outputs if name not in ("help", "no input") else arguments
            old, old_files, _ = run(baseline, arguments)
            new, new_files, _ = run(GENERATOR, arguments)
            assert old.returncode == new.returncode, name
            if name == "no input":
                # Tracebacks include source paths and changed line numbers; retain the error contract.
                assert old.returncode != 0 and old.stderr.splitlines()[-1:] == new.stderr.splitlines()[-1:], name
            else:
                assert old.returncode == 0, (name, old.stderr.decode())
                assert stderr_without_locations(old.stderr, baseline) == stderr_without_locations(new.stderr, GENERATOR), (name, "stderr")
            assert old.stdout == new.stdout, (name, "stdout")
            assert old_files == new_files, (name, "generated files")
            print(f"PASS output comparison (stderr source locations normalized): {name}")
        if benchmark:
            timings = {"eager": [], "lazy": []}
            arguments = cases[0][1] + outputs
            for index in range(runs):
                order = [("eager", baseline), ("lazy", GENERATOR)]
                if index % 2:
                    order.reverse()
                for name, script in order:
                    result, _, elapsed = run(script, arguments)
                    assert result.returncode == 0, result.stderr.decode()
                    timings[name].append(elapsed)
            old = statistics.median(timings["eager"])
            new = statistics.median(timings["lazy"])
            print(f"BENCHMARK {runs} interleaved cold runs each: eager median {old:.4f}s; "
                  f"lazy median {new:.4f}s; {(old - new) / old * 100:.1f}% reduction")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", default=sys.executable, help="Python with installed color dependencies")
    parser.add_argument("--benchmark", action="store_true")
    parser.add_argument("--runs", type=int, default=7)
    options = parser.parse_args()
    if options.runs < 1:
        parser.error("--runs must be positive")
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(ImportDemandTests))
    if not result.wasSuccessful():
        sys.exit(1)
    real_checks(options.python, options.benchmark, options.runs)
