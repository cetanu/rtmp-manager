"""Exercise generic Rhai automations below chat, including real stream live gating."""
import argparse
import re
import subprocess
import time

from playwright.sync_api import expect, sync_playwright
from chat_layout import dashboard


def wait_status(context, base_url, allowed):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        state = context.request.get(base_url + "/api/stream/status").json()["state"]
        if state in allowed:
            return state
        time.sleep(0.1)
    raise AssertionError(f"Stream status was {state}, expected {allowed}")


def check_automations(browser, base_url, viewport):
    context = browser.new_context(viewport=viewport)
    context.route("https://fonts.googleapis.com/**", lambda route: route.abort())
    context.route("https://fonts.gstatic.com/**", lambda route: route.abort())
    page = context.new_page()
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    stream = None
    try:
        page.goto(base_url + "/chat")
        panel = page.locator("#chat-automations")
        inbox = page.locator("[data-chat-inbox]")
        # The automations are a sibling section below the complete chat panel.
        assert panel.evaluate("node => !node.closest('[data-chat-inbox]')")
        assert panel.bounding_box()["y"] >= inbox.bounding_box()["y"] + inbox.bounding_box()["height"]
        listing = panel.locator("[data-automation-list]").bounding_box()
        log = panel.locator("[data-automation-log]").bounding_box()
        if viewport["width"] >= 768:
            assert log["x"] >= listing["x"] + listing["width"]
            assert abs(log["y"] - listing["y"]) < 1
        else:
            assert log["y"] >= listing["y"] + listing["height"]
        panel.get_by_role("button", name="New automation", exact=True).click()
        editor = page.get_by_role("dialog", name="New automation", exact=True)
        expect(editor).to_be_visible()
        expect(editor.get_by_label("Automation name", exact=True)).to_be_focused()
        page.keyboard.press("Escape")
        expect(editor).not_to_be_visible()
        panel.get_by_role("button", name="New automation", exact=True).click()
        editor.get_by_role("button", name="Cancel", exact=True).click()
        expect(editor).not_to_be_visible()
        panel.get_by_role("button", name="New automation", exact=True).click()
        editor.get_by_label("Automation name", exact=True).fill("Points")
        editor.get_by_label("Rhai script", exact=True).fill(
            'let points = if viewer.contains("points") { viewer.points } else { 0 }; '
            'viewer.points = points + 1; '
            'message.author + " has " + viewer.points.to_string() + " points"'
        )
        panel.get_by_label("Automation enabled", exact=True).check()
        panel.get_by_label("Message pattern", exact=True).fill("[")
        panel.get_by_role("button", name="Save", exact=True).click()
        expect(editor.get_by_role("alert")).to_contain_text(re.compile("pattern", re.IGNORECASE))
        expect(editor).to_be_visible()
        panel.get_by_label("Message pattern", exact=True).fill("(?i)^!point$")
        panel.get_by_role("button", name="Save", exact=True).click()
        expect(editor).not_to_be_visible()
        row = panel.locator("[data-automation-id]").first
        expect(row.get_by_role("img", name="Enabled · no active stream", exact=True)).to_be_visible()
        expect(row.get_by_role("button")).to_have_text(["Edit", "Disable", "Delete"])
        expect(row.locator("code")).to_have_count(0)
        expect(row.get_by_text("(?i)^!point$", exact=True)).not_to_be_visible()
        dot = row.get_by_role("img")
        heading = row.get_by_role("heading", name="Points", exact=True)
        assert dot.bounding_box()["x"] < heading.bounding_box()["x"]
        expect(dot).to_have_class(re.compile("bg-warn"))

        def send(text="!point"):
            response = context.request.post(base_url + "/api/chat/test", data={
                "source": "twitch", "author": "Test viewer", "user_id": "viewer-123", "text": text,
            })
            assert response.ok, response.text()
            return response.json()

        send()
        expect(panel).to_contain_text("No executions yet")
        config = context.request.get(base_url + "/api/config").json()
        target = f"rtmp://{config['server']['listen']}/live/local-ingest"
        stream = subprocess.Popen([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-re",
            "-f", "lavfi", "-i", "color=c=black:s=320x180:r=10",
            "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono",
            "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency",
            "-pix_fmt", "yuv420p", "-g", "20", "-c:a", "aac", "-f", "flv", target,
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        wait_status(context, base_url, {"preparing", "preview_ready", "preview_failed"})
        send()
        expect(panel).to_contain_text("No executions yet")
        preview = context.new_page()
        preview.goto(base_url + "/preview")
        preview.get_by_role("button", name="PUBLISH", exact=True).click()
        wait_status(context, base_url, {"live"})
        expect(row.get_by_role("img", name="Enabled · live", exact=True)).to_be_visible()
        expect(dot).to_have_class(re.compile("bg-signal"))
        send(" !POINT ")
        expect(panel).to_contain_text("Test viewer has 1 points")
        send()
        expect(panel).to_contain_text("Test viewer has 2 points")
        # Execution updates must preserve an open editor and its draft.
        row.get_by_role("button", name="Edit", exact=True).click()
        editor = page.get_by_role("dialog", name="Edit automation", exact=True)
        editor.get_by_label("Automation name", exact=True).fill("Unsaved draft")
        send()
        expect(panel).to_contain_text("Test viewer has 3 points")
        expect(editor).to_be_visible()
        expect(editor.get_by_label("Automation name", exact=True)).to_have_value("Unsaved draft")
        assert editor.evaluate("node => node.matches(':modal')")
        bounds = editor.bounding_box()
        assert bounds["width"] <= viewport["width"]
        assert bounds["height"] <= viewport["height"]
        editor.get_by_role("button", name="Cancel", exact=True).click()
        row.get_by_role("button", name="Disable", exact=True).click()
        expect(row.get_by_role("img", name="Disabled", exact=True)).to_be_visible()
        expect(dot).to_have_class(re.compile("bg-live"))
        send()
        expect(panel).not_to_contain_text("Test viewer has 4 points")
        # Editing retains stored data and does not re-enable a disabled automation.
        row.get_by_role("button", name="Edit", exact=True).click()
        editor = page.get_by_role("dialog", name="Edit automation", exact=True)
        expect(editor).to_be_visible()
        editor.get_by_label("Rhai script", exact=True).fill("viewer.points += 10; viewer.points.to_string()")
        editor.get_by_role("button", name="Save", exact=True).click()
        expect(editor).not_to_be_visible()
        expect(row.get_by_role("img", name="Disabled", exact=True)).to_be_visible()
        expect(dot).to_have_class(re.compile("bg-live"))
        row.get_by_role("button", name="Enable", exact=True).click()
        send()
        expect(panel.get_by_text("13", exact=True)).to_be_visible()
        page.reload()
        expect(panel).to_contain_text("Points")
        expect(panel.get_by_text("13", exact=True)).to_be_visible()
        preview.get_by_role("button", name="LIVE", exact=True).click()
        wait_status(context, base_url, {"preparing", "preview_ready", "preview_failed"})
        expect(row.get_by_role("img", name="Enabled · no active stream", exact=True)).to_be_visible()
        send()
        expect(panel.get_by_text("23", exact=True)).to_have_count(0)
        assert not errors, errors
        row.get_by_role("button", name="Delete", exact=True).click()
        expect(panel.locator("[data-automation-id]")).to_have_count(0)
        assert not errors, errors
        print(f"PASS: chat automations at {viewport['width']}×{viewport['height']}")
    finally:
        if stream:
            stream.terminate()
            try:
                stream.wait(timeout=5)
            except subprocess.TimeoutExpired:
                stream.kill()
                stream.wait()
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
                    check_automations(browser, base_url, viewport)
        finally:
            browser.close()


if __name__ == "__main__":
    main()
