import ns4_fx_crosscheck as xc


def fx(**kw):
    base = dict(
        mod1_type_raw=2,
        mod2_type_raw=7,
        reverb_type_raw=3,
        amp_type_new=33,
        reverb_color=0,
        delay_ping_pong=False,
        delay_fb_filter=True,
        delay_mode=0,
        delay_fb_fx_type=7,
    )
    base.update(kw)
    return base


def test_predict_matches_known_jazz_guitar_block():
    p = xc.predict(fx())
    assert p["FX mod 1 mode"] == "TREM"
    assert p["FX mod 2 mode"] == "VIBE variation"
    assert p["FX reverb type"] == "STAGE"
    assert p["FX delay filter type"] == "LP"
    assert p["FX delay effects"] == "CHOR variation"
    assert p["FX amp sim/EQ mode"] == "TWIN"


def test_predict_reverb_variation_bit_comes_from_amp_field_top_bit():
    assert xc.predict(fx(amp_type_new=0x100 | 33))["FX reverb type"] == "STAGE variation"
    assert xc.predict(fx(reverb_type_raw=0, amp_type_new=0))["FX reverb type"] == "SPRING"


def test_predict_delay_bits_and_unknown_values():
    p = xc.predict(fx(delay_mode=0b11, delay_ping_pong=True, delay_fb_filter=True))
    assert p["FX delay normal/analog"] == "analog" and p["FX delay ping pong on/off"] == "on"
    assert p["FX delay filter type"] == "BP"
    assert xc.predict(fx(mod2_type_raw=15))["FX mod 2 mode"] == "?"


def test_read_csv_and_main_usage(tmp_path, capsys):
    f = tmp_path / "a.csv"
    f.write_text("parameter,layer A" + chr(10) + "FX mod 1 mode,TREM" + chr(10), encoding="utf-8")
    assert xc.read_csv(f) == {"parameter": ["layer A"], "FX mod 1 mode": ["TREM"]}
    assert xc.main([]) == 2
