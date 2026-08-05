from alens import settings


def test_defaults_do_not_contact_external_hub(monkeypatch):
    monkeypatch.delenv("A_LENS_SOURCE", raising=False)
    monkeypatch.delenv("A_LENS_WORK_URL", raising=False)

    cfg = settings._defaults()

    assert cfg["source"] == "dummy"
    assert cfg["work_url"] == ""


def test_explicit_hub_settings_override_safe_defaults(monkeypatch):
    monkeypatch.setenv("A_LENS_SOURCE", "hub")
    monkeypatch.setenv("A_LENS_WORK_URL", "https://hub.example.com")

    cfg = settings._defaults()

    assert cfg["source"] == "hub"
    assert cfg["work_url"] == "https://hub.example.com"
