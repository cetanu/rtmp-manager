# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.5](https://github.com/cetanu/rtmp-manager/compare/v0.3.4...v0.3.5) - 2026-08-31

### Added

- *(kick)* manage chat webhook subscriptions

### Fixed

- *(ui)* label webhook controls accurately

### Other

- add product engineering roadmap
- *(kick)* add webhook settings to example

## [0.3.4](https://github.com/cetanu/rtmp-manager/compare/v0.3.3...v0.3.4) - 2026-08-31

### Added

- *(logging)* log webhook activity

## [0.3.3](https://github.com/cetanu/rtmp-manager/compare/v0.3.2...v0.3.3) - 2026-08-31

### Added

- *(chat)* ingest X broadcast webhooks
- *(chat)* add Kick chat toggle
- *(webhooks)* broadcast inbound events
- *(ui)* display version in navigation

### Fixed

- *(ci)* configure release-plz for git-only releases
- *(security)* replace vulnerable RSA verifier
- *(ci)* specify release repository

### Other

- *(chat)* remove external ingest endpoint

## [0.3.2](https://github.com/cetanu/rtmp-manager/releases/tag/v0.3.2) - 2026-08-30

### Added

- rework YouTube chat polling

### Fixed

- *(deps)* update h2 security patch
- *(ci)* run cargo audit without lock flag
- fix chat polling toggle buttons
- fix publishing/hls preview
- fix test stream button
- fix caddy config
- fix paths

### Other

- align package version with release tag
- automate releases with release-plz
- more refactor. destruction of the shitty arc mutex spam
- refactor
- remove "empty_state" box
- bump
- the great deslopification
- Release v0.1.19
- Redesign chat inbox queue
- remove caddy
- support X live chat
- Add YouTube chat polling toggle
- tokio toasty and serde valid instead of raw sqlite
- lints and cleanup
- better diagnostics from ffmpeg errors
- hide keys from logs, test streams bypass publish lifecycle
- update config
- lints
- allow viewing and editing secrets but hide by default
- metrics page with charts
- add logs page
- split up salt states
- split up web app into pages
- manage ffmpeg install
- optimize binary size
- add websockets for chat and preview
- upgrade toolchain
- lock down the stream key
- add stream preview before publishing to targets
- ensure bundle assets are included and installed
- add environment pillar dummy
- trigger deployments after release upload
- add json config
- add encrypted Git pillar contract
- initial rtmp-manager application
