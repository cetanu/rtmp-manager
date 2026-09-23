CREATE TABLE "chat_poll" (
    "id" INTEGER NOT NULL,
    "question" TEXT NOT NULL,
    "options_data" TEXT NOT NULL,
    "started_at_unix_ms" INTEGER NOT NULL,
    "stopped_at_unix_ms" INTEGER,
    "results_ends_at_unix_ms" INTEGER,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE TABLE "chat_poll_votes" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "poll_id" INTEGER NOT NULL,
    "voter_key" TEXT NOT NULL,
    "option_number" INTEGER NOT NULL
);
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_chat_poll_votes_by_poll_and_voter"
    ON "chat_poll_votes" ("poll_id", "voter_key");
