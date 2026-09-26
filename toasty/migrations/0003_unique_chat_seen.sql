DROP INDEX "index_chat_seen_by_source_and_external_id";
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_chat_seen_by_source_and_external_id" ON "chat_seen" ("source", "external_id");
