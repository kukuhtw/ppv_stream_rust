CREATE TABLE IF NOT EXISTS chat_conversations (
  id TEXT PRIMARY KEY,
  conversation_type TEXT NOT NULL,
  direct_user_a_id TEXT NULL REFERENCES users(id) ON DELETE CASCADE,
  direct_user_b_id TEXT NULL REFERENCES users(id) ON DELETE CASCADE,
  support_user_id TEXT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_by_user_id TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_message_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS chat_messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES chat_conversations(id) ON DELETE CASCADE,
  sender_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  body TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_chat_direct_unique
  ON chat_conversations (direct_user_a_id, direct_user_b_id)
  WHERE conversation_type = 'direct';

CREATE UNIQUE INDEX IF NOT EXISTS idx_chat_support_unique
  ON chat_conversations (support_user_id)
  WHERE conversation_type = 'admin_support';

CREATE INDEX IF NOT EXISTS idx_chat_conversations_support_user
  ON chat_conversations (support_user_id, last_message_at DESC);

CREATE INDEX IF NOT EXISTS idx_chat_conversations_direct_a
  ON chat_conversations (direct_user_a_id, last_message_at DESC);

CREATE INDEX IF NOT EXISTS idx_chat_conversations_direct_b
  ON chat_conversations (direct_user_b_id, last_message_at DESC);

CREATE INDEX IF NOT EXISTS idx_chat_messages_conversation_created
  ON chat_messages (conversation_id, created_at ASC);
