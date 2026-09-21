CREATE TABLE "chat_pomodoro" (
    "id" INTEGER NOT NULL,
    "message" TEXT NOT NULL,
    "started_at_unix_ms" INTEGER NOT NULL,
    "ends_at_unix_ms" INTEGER NOT NULL,
    PRIMARY KEY ("id")
);
