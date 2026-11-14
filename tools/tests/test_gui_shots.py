import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import gui_shots as g  # noqa: E402


def test_shot_command_names_section_and_caps_keybed_scale():
    cmd = g.shot_command("exe", "out", "organ", 2.0)
    assert cmd[0] == "exe"
    assert cmd[cmd.index("--screenshot-section") + 1] == "organ"
    assert cmd[cmd.index("--screenshot-scale") + 1] == "2.0"
    assert cmd[2].endswith("organ.png")
    key = g.shot_command("exe", "out", "keybed", 2.0)
    assert key[key.index("--screenshot-scale") + 1] == "1.0"


def test_run_shot_kills_a_hung_child_and_reports_exit_codes():
    sleeper = [sys.executable, "-c", "import time; time.sleep(30)"]
    assert g.run_shot(sleeper, timeout=1) is False
    assert g.run_shot([sys.executable, "-c", "pass"]) is True
    assert g.run_shot([sys.executable, "-c", "raise SystemExit(3)"]) is False


def test_sections_cover_the_panel():
    assert {"organ", "piano", "program", "synth", "effects"} <= set(g.SECTIONS)
