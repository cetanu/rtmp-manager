"""Chat must fill the viewport and scroll messages inside the panel.

Run after building both binaries and bundling assets; see README.md.
"""

import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

from playwright.sync_api import expect, sync_playwright


ROOT = Path(__file__).resolve().parents[2]


def unused_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


@contextmanager
def dashboard(profile):
    """Use the real server with an isolated database and local listeners."""
    binaries = ROOT / "target" / profile
    with tempfile.TemporaryDirectory(prefix="rtmp-chat-layout-") as directory:
        database = Path(directory) / "config.sqlite3"
        subprocess.run(
            [str(binaries / "migrate"), "migration", "apply"],
            cwd=ROOT,
            env={**os.environ, "CONFIG_PATH": str(database)},
            check=True,
            capture_output=True,
        )
        config = json.loads((ROOT / "tests/browser/config.json").read_text())
        web_port = unused_port()
        ingest_port = unused_port()
        while ingest_port == web_port:
            ingest_port = unused_port()
        config["server"]["api_listen"] = f"127.0.0.1:{web_port}"
        config["server"]["listen"] = f"127.0.0.1:{ingest_port}"
        config["web_auth"] = {"overlay_token": "browser-test-overlay-token"}
        config["targets"] = []
        config["chat"] = {"queue_mode": True}
        with sqlite3.connect(database) as connection:
            connection.execute(
                "INSERT INTO app_config (id, data) VALUES (?, ?)",
                (1, json.dumps(config)),
            )
        base_url = f"http://127.0.0.1:{web_port}"
        with tempfile.TemporaryFile(mode="w+") as log:
            server = subprocess.Popen(
                [str(binaries / "rtmp-proxy"), "--config", str(database)],
                cwd=ROOT,
                stdout=log,
                stderr=log,
            )
            try:
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    if server.poll() is not None:
                        raise RuntimeError("dashboard exited during startup")
                    try:
                        with urllib.request.urlopen(base_url + "/api/config", timeout=1):
                            break
                    except (urllib.error.URLError, TimeoutError):
                        time.sleep(0.1)
                else:
                    raise TimeoutError("dashboard did not start within 15 seconds")
                yield base_url
            except Exception:
                log.seek(0)
                print(log.read())
                raise
            finally:
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()


def assert_fills_viewport(page):
    # Measure the rendered result, independent of class names and CSS selectors.
    geometry = page.locator("[data-chat-inbox]").evaluate("""panel => {
        const main = document.querySelector('main');
        const messages = panel.querySelector('[data-chat-messages]');
        const padding = getComputedStyle(main);
        const mainRect = main.getBoundingClientRect();
        const panelRect = panel.getBoundingClientRect();
        return {
            viewport: innerHeight,
            mainBottom: mainRect.bottom,
            available: main.clientHeight - parseFloat(padding.paddingTop)
                - parseFloat(padding.paddingBottom),
            panelHeight: panelRect.height,
            messageHeight: messages.clientHeight,
            messageScrollHeight: messages.scrollHeight,
        };
    }""")
    assert abs(geometry["mainBottom"] - geometry["viewport"]) <= 2, geometry
    assert abs(geometry["panelHeight"] - geometry["available"]) <= 2, geometry
    assert geometry["messageHeight"] >= geometry["available"] * 0.65, geometry
    return geometry


def check_chat_layout(browser, base_url, viewport):
    context = browser.new_context(viewport=viewport)
    # Font availability must not determine whether this layout regression passes.
    context.route("https://fonts.googleapis.com/**", lambda route: route.abort())
    context.route("https://fonts.gstatic.com/**", lambda route: route.abort())
    page = context.new_page()
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    try:
        response = page.goto(base_url + "/chat")
        assert response.status == 200
        expect(page.locator("[data-chat-inbox]")).to_be_visible()
        assert_fills_viewport(page)

        # The same layout must be applied when the runtime changes pages.
        page.evaluate("window.chatLayoutDocument = true")
        page.get_by_role("link", name="Settings", exact=True).click()
        page.wait_for_url("**/settings")
        page.get_by_role("link", name="Chat", exact=True).click()
        page.wait_for_url("**/chat")
        expect(page.locator("[data-chat-inbox]")).to_be_visible()
        assert page.evaluate("window.chatLayoutDocument === true")
        assert_fills_viewport(page)

        # Enough real messages to overflow even a large desktop viewport.
        for index in range(10):
            response = context.request.post(base_url + "/api/chat/test", data={
                "source": "twitch",
                "author": f"Layout viewer {index}",
                "text": "A long chat message exercises scrolling inside the inbox. " * 70,
            })
            assert response.ok, response.text()
        expect(page.locator("[data-chat-messages] article")).to_have_count(10)
        geometry = assert_fills_viewport(page)
        assert geometry["messageScrollHeight"] > geometry["messageHeight"], geometry

        # Focus controls must switch on the live page, including another tab.
        other = context.new_page()
        other.goto(base_url + "/chat")
        page.locator("#chat-pomodoro-focus").click()
        for tab in (page, other):
            expect(tab.locator("#chat-pomodoro-stop")).to_be_visible()
            expect(tab.locator("#chat-pomodoro-focus")).to_be_hidden()
        page.locator("#chat-pomodoro-stop").click()
        for tab in (page, other):
            expect(tab.locator("#chat-pomodoro-focus")).to_be_visible()
            expect(tab.locator("#chat-pomodoro-stop")).to_be_hidden()
        other.close()

        # The OBS viewport follows the latest message as content and size change.
        page.get_by_role("switch", name="Toggle Queue incoming chat", exact=True).click()
        overlay_url = page.get_by_role("link", name="OBS Overlay").get_attribute("href")
        overlay = context.new_page()
        for direction in ("down", "up"):
            overlay.goto(base_url + overlay_url + "&direction=" + direction)
            expect(overlay.locator(".chat-overlay-message")).to_have_count(10)
            def assert_latest_visible():
                overlay.wait_for_function("""() => {
                    const rows = document.querySelectorAll('.chat-overlay-message');
                    const rect = rows[rows.length - 1].getBoundingClientRect();
                    const reversed = getComputedStyle(document.getElementById('chat-overlay-messages')).flexDirection === 'column-reverse';
                    return reversed ? rect.top >= 0 && rect.top < innerHeight
                        : rect.bottom > 0 && rect.bottom <= innerHeight;
                }""")
            assert_latest_visible()
            response = context.request.post(base_url + "/api/chat/test", data={
                "source": "twitch", "author": "Latest viewer",
                "text": "The latest message must stay visible.",
            })
            assert response.ok, response.text()
            expect(overlay.locator(".chat-overlay-message")).to_have_count(11)
            assert_latest_visible()
            overlay.set_viewport_size({**viewport, "height": 400})
            assert_latest_visible()
            # Restore the message count before testing the other direction.
            page.get_by_role("button", name="Clear all", exact=True).click()
            expect(overlay.locator(".chat-overlay-message")).to_have_count(0)
            for index in range(10):
                response = context.request.post(base_url + "/api/chat/test", data={
                    "source": "twitch", "author": f"Viewer {index}",
                    "text": "Overflowing overlay message. " * 70,
                })
                assert response.ok, response.text()
        overlay.close()

        # Resizing and navigating back must not leave the panel at its old height.
        page.set_viewport_size({**viewport, "height": viewport["height"] + 180})
        assert_fills_viewport(page)
        page.get_by_role("link", name="Settings", exact=True).click()
        page.wait_for_url("**/settings")
        page.go_back()
        page.wait_for_url("**/chat")
        expect(page.locator("[data-chat-inbox]")).to_be_visible()
        assert_fills_viewport(page)
        assert not errors, errors
        print(f"PASS: chat viewport layout at {viewport['width']}×{viewport['height']}")
    finally:
        context.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release"), default="debug")
    args = parser.parse_args()
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        try:
            for viewport in ({"width": 1440, "height": 900}, {"width": 390, "height": 844}):
                with dashboard(args.profile) as base_url:
                    check_chat_layout(browser, base_url, viewport)
        finally:
            browser.close()


if __name__ == "__main__":
    main()
