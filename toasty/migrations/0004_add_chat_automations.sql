CREATE TABLE "chat_automation_runs" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "automation_id" INTEGER NOT NULL,
    "automation_name" TEXT NOT NULL,
    "source" TEXT NOT NULL,
    "external_id" TEXT NOT NULL,
    "author" TEXT NOT NULL,
    "output" TEXT NOT NULL,
    "error" TEXT NOT NULL,
    "received_at_unix_ms" INTEGER NOT NULL
);
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_automation_run_message" ON "chat_automation_runs" ("automation_id", "source", "external_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_automation_message" ON "chat_automation_runs" ("source", "external_id");
-- #[toasty::breakpoint]
CREATE TABLE "chat_automation_data" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "automation_id" INTEGER NOT NULL,
    "scope" TEXT NOT NULL,
    "data" TEXT NOT NULL
);
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_automation_data_scope" ON "chat_automation_data" ("automation_id", "scope");
-- #[toasty::breakpoint]
CREATE TABLE "chat_automations" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "pattern" TEXT NOT NULL,
    "script" TEXT NOT NULL,
    "enabled" BOOLEAN NOT NULL,
    "consume" BOOLEAN NOT NULL
);
