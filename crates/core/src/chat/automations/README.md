## Chat automations

The **Chat automations** section below the chat panel lets you define a message
pattern and a Rhai script. There are no built-in giveaway rules: the dice
example is a script you can edit, just like the points example.

Each automation has a name, a regular expression, a script, an enabled flag and
an option to hide successfully handled messages from the inbox. Patterns search
the trimmed message; use anchors for a whole-message match, e.g.
`(?i)^!roll$`. All matching enabled automations run in creation order. A message
runs each script once, even if its pattern matches several places.

**Automations run only while the stream status is Live.** Offline, preparing,
preview and preview-failed states do not execute scripts. Enabling an automation
while offline leaves it waiting for the next live stream. Unhandled messages
continue through poll voting and the display inbox as usual.

Scripts receive:

| Value | Meaning |
| --- | --- |
| `message.platform` | `twitch`, `youtube`, `kick` or `x` |
| `message.user_id` | Platform account ID, or an empty string if unavailable |
| `message.author`, `message.text`, `message.id` | Display name, trimmed text and platform message ID |
| `captures` | Full regex match at index 0, then captured groups; unmatched optional groups are empty strings |
| `now_ms` | Current Unix time in milliseconds |
| `state` | Persistent object shared by this automation |
| `viewer` | Persistent object for this automation and platform account |

For example, pattern `(?i)^!point$` can run:

```rhai
if message.user_id == "" { throw "A platform user ID is required"; }
let points = if viewer.contains("points") { viewer.points } else { 0 };
viewer.points = points + 1;
message.author + " has " + viewer.points.to_string() + " points"
```

`random_int(n)` produces an unbiased random integer from 1 through n. Return a
string to show a result under **Recent executions**, or `()` for no output.
Scripts can use counters, strings, booleans and nested JSON data. Global and
viewer objects are each limited to 64 KiB. Writing viewer data requires a
platform account ID; a renamed account keeps its data, and accounts on different
platforms are separate.

State changes commit with durable execution records. Duplicate platform message
IDs cannot repeat script effects, including after a restart or after the inbox
has discarded its old seen IDs. A script error is recorded and discards that
script's state changes; other matching scripts still run. Failed scripts do not
hide messages from the inbox. Editing or disabling an automation keeps its data;
deleting it removes its data and retains execution history for deduplication.
The page displays the latest 20 executions; SQLite retains the full history.

Rhai has no filesystem, network or module access here. Execution, recursion,
collection sizes and output length are bounded. Scripts run on the blocking pool.
Output currently appears in the dashboard; scripts do not send platform chat
replies. The implementation is in `crates/core/src/chat/automations/`, with script evaluation
separate from storage and execution coordination.

### Script alerts and media

Open **Media** in the app navigation. Upload
images (PNG, GIF, JPEG, WebP), MP4 videos or sounds (MP3, WAV, Ogg), up to 10 MiB per file
and 100 files total. Files live in the shared SQLite database. Names are unique
and may contain ASCII letters, numbers, dots, dashes and underscores.

Call `overlay.show` from any script branch:

```rhai
if rolled == 1 {
    overlay.show(#{
        text: message.author + " WON!",
        image: "jackpot.gif",
        sound: "jackpot.mp3",
        position: #{x: 70, y: 10, width: 25, height: 30},
        duration_ms: 8000
    });
    message.author + " WON!"
} else {
    message.author + " rolled " + rolled.to_string()
}
```

Text, image, video and sound are optional, but at least one must be supplied. Use
`video: "clip.mp4"` to play an MP4 with its embedded audio. Playback stops at
the alert duration; use a duration long enough to play the clip.
`duration_ms` is required (250–60,000 ms); each execution may emit up to eight
alerts. Media references must exist and match the requested image, video or sound type.
An invalid alert fails the script and discards its state changes and alerts.
Deleting media makes scripts that reference it fail until their references are
updated or the file is uploaded again.

Set the OBS Browser Source width and height to match your stream canvas, for
example 1920 × 1080 for 1080p, and place it at the canvas origin. The optional
`position` defines a rectangle as integer percentages of that canvas: `x` and
`y` are its top-left corner, and `width` and `height` are its size. All four
values are required; dimensions must be positive and the rectangle must fit
inside the canvas. For example, the rectangle above starts 70% from the left
and 10% from the top. Text and media start at the rectangle's top-left corner;
images and videos keep their aspect ratio inside it. Text and visual media in
the same alert share the rectangle. Omit
`position` to use the whole canvas. Each `overlay.show` call can use a different
rectangle.

Add `/overlay/alerts?key=YOUR_OVERLAY_TOKEN` as a separate OBS Browser Source,
using **OBS overlay**. Alerts play sequentially, with audio stopped at
the end of each alert. Enable **Control audio via OBS** to mix it through OBS.
Alerts are published only after the execution commits. They are live events:
disconnected overlays miss alerts, and refreshing or reconnecting never replays
old alerts. Each connected browser source plays its own copy. A slow client has
a bounded queue (64 waiting alerts); additional alerts are dropped.

**Send test message** runs the editor draft with fresh state, even while offline,
and shows its output, state and alerts without saving anything. Test alerts play
on every connected OBS overlay, including any visible in a live scene. Open the
overlay before testing to see and hear them. Media references are checked before
playback.

For local testing while live, POST JSON to the authenticated `/api/chat/test`
endpoint, for example:
`{"source":"twitch","author":"Test viewer","user_id":"test-viewer","text":"!point"}`.
Offline test messages only enter the inbox and cannot change script data.
