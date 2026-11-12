import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import nord_os_urls as n  # noqa: E402


def test_known_url_matches_vendor_pattern():
    assert (
        n.url("1.46")
        == "https://www.nordkeyboards.com/wt/documents/783/Update%20Nord%20Stage%204%20OS%20v1.46.zip"
    )
    assert n.url("1.66", "dmg").endswith("/1041/Update%20Nord%20Stage%204%20OS%20v1.66.dmg")


def test_unknown_id_and_version_return_none():
    assert n.url("9.99") is None
    assert n.url("1.66", "exe") is None


def test_ids_rise_with_version_and_probes_exclude_known_ids():
    known = sorted((v, d[0]) for v, d in n.DOCS.items() if d[0] is not None)
    ids = [i for _, i in known]
    assert ids == sorted(ids)
    for ver, lo, hi in n.PROBED:
        doc = n.DOCS[ver][0]
        assert doc is None or not lo <= doc <= hi


def test_main_lists(capsys):
    assert n.main(["1.52"]) == 0
    assert "documents/899/" in capsys.readouterr().out
