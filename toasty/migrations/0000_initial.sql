CREATE TABLE "chat_messages" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "source" TEXT NOT NULL,
    "external_id" TEXT NOT NULL,
    "author" TEXT NOT NULL,
    "text" TEXT NOT NULL,
    "emoji_data" TEXT,
    "avatar_url" TEXT,
    "sent_at" TEXT,
    "received_at_unix_ms" INTEGER NOT NULL
);
-- #[toasty::breakpoint]
CREATE TABLE "chat_state" (
    "id" INTEGER NOT NULL,
    "dropped" INTEGER NOT NULL,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE TABLE "app_config" (
    "id" BIGINT NOT NULL,
    "data" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE TABLE "chat_seen" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "source" TEXT NOT NULL,
    "external_id" TEXT NOT NULL
);
-- #[toasty::breakpoint]
CREATE INDEX "index_chat_seen_by_source_and_external_id" ON "chat_seen" ("source", "external_id");
