"""Deployment choices must respect the APIs used by the mobile fixtures."""
import argparse
import contextlib
import importlib.util
import io
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("build_mobile", Path(__file__).parents[1] / "build-mobile.py")
mobile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mobile)


class MobileChoicesTests(unittest.TestCase):
    def choices(self, **overrides):
        values = dict(platform="android", deployment_target=None, android_min_api=26,
                      android_target_sdk=36, android_compile_sdk=36, android_build_tools="36.0.0")
        values.update(overrides)
        return argparse.Namespace(**values)

    def validate(self, **overrides):
        args = self.choices(**overrides)
        with contextlib.redirect_stderr(io.StringIO()):
            mobile.validate_choices(argparse.ArgumentParser(), args)
        return args

    def test_consumer_android_versions(self):
        self.validate(android_min_api=28, android_target_sdk=35)
        self.validate(android_min_api=36, android_target_sdk=37, android_compile_sdk=37)

    def test_android_incompatible_versions(self):
        for choices in (dict(android_min_api=25), dict(android_min_api=37),
                        dict(android_target_sdk=37), dict(android_compile_sdk=35),
                        dict(deployment_target="18.0"), dict(android_build_tools="../tools")):
            with self.subTest(choices=choices), self.assertRaises(SystemExit):
                self.validate(**choices)

    def test_apple_defaults_and_consumer_versions(self):
        for platform, default in (("macos", "12.0"), ("ios", "17.0"), ("ios-sim", "17.0")):
            with self.subTest(platform=platform):
                self.assertEqual(self.validate(platform=platform).deployment_target, default)
                self.assertEqual(self.validate(platform=platform, deployment_target="18.2").deployment_target, "18.2")

    def test_apple_incompatible_versions(self):
        for platform, version in (("macos", "11.9"), ("ios", "16.9"), ("ios-sim", "16"),
                                  ("ios", "17.beta"), ("ios", "17..0"), ("ios", "١٧")):
            with self.subTest(platform=platform, version=version), self.assertRaises(SystemExit):
                self.validate(platform=platform, deployment_target=version)


if __name__ == "__main__":
    unittest.main()
