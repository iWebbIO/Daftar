-- دفتر — schema v1
--
-- Money is stored as integer toman. Quantities are stored as thousandths
-- (qty_milli), so 2.5 kg is 2500 and nothing rounds unexpectedly.
--
-- A date is a Julian Day Number (`day`), an integer that sorts and compares
-- correctly with no timezone attached. Jalali is how it is shown, not how it
-- is stored. `at` columns are epoch milliseconds.

CREATE TABLE IF NOT EXISTS party (
  id              INTEGER PRIMARY KEY,
  name            TEXT    NOT NULL,
  phone           TEXT,
  kind            TEXT    NOT NULL DEFAULT 'customer', -- customer | supplier | both
  address         TEXT,
  note            TEXT,
  opening         INTEGER NOT NULL DEFAULT 0,          -- positive: they owe me
  archived        INTEGER NOT NULL DEFAULT 0,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS party_name ON party(name);

CREATE TABLE IF NOT EXISTS product (
  id              INTEGER PRIMARY KEY,
  name            TEXT    NOT NULL,
  unit            TEXT    NOT NULL DEFAULT 'kg',       -- kg | piece | jar | bag | ...
  sale_price      INTEGER NOT NULL DEFAULT 0,          -- last price sold at
  buy_price       INTEGER NOT NULL DEFAULT 0,          -- last price bought at
  active          INTEGER NOT NULL DEFAULT 1,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS product_name ON product(name);

-- Every price ever typed, so "what did I charge for this last time" is a read
-- rather than a memory.
CREATE TABLE IF NOT EXISTS price_history (
  id              INTEGER PRIMARY KEY,
  product_id      INTEGER NOT NULL REFERENCES product(id),
  party_id        INTEGER REFERENCES party(id),
  kind            TEXT    NOT NULL,                    -- sale | buy
  price           INTEGER NOT NULL,
  day             INTEGER NOT NULL,
  at              INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS price_lookup ON price_history(product_id, party_id, kind, at DESC);

-- One invoice (sale) or one purchase. `number` is per Jalali year.
CREATE TABLE IF NOT EXISTS doc (
  id              INTEGER PRIMARY KEY,
  kind            TEXT    NOT NULL,                    -- sale | purchase
  jyear           INTEGER NOT NULL,
  number          INTEGER NOT NULL,
  party_id        INTEGER NOT NULL REFERENCES party(id),
  day             INTEGER NOT NULL,
  status          TEXT    NOT NULL DEFAULT 'open',     -- open | settled | void
  discount        INTEGER NOT NULL DEFAULT 0,
  rounding        INTEGER NOT NULL DEFAULT 0,          -- amount the total was rounded by
  note            TEXT,
  total           INTEGER NOT NULL DEFAULT 0,
  final           INTEGER NOT NULL DEFAULT 0,          -- the circled total
  void_reason     TEXT,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS doc_number ON doc(kind, jyear, number);
CREATE INDEX IF NOT EXISTS doc_party ON doc(party_id, day DESC);
CREATE INDEX IF NOT EXISTS doc_day ON doc(day DESC);

CREATE TABLE IF NOT EXISTS doc_line (
  id              INTEGER PRIMARY KEY,
  doc_id          INTEGER NOT NULL REFERENCES doc(id) ON DELETE CASCADE,
  pos             INTEGER NOT NULL,
  product_id      INTEGER REFERENCES product(id),
  name            TEXT    NOT NULL,
  qty_milli       INTEGER NOT NULL,
  unit            TEXT    NOT NULL,
  price           INTEGER NOT NULL,
  total           INTEGER NOT NULL,
  override_total  INTEGER,                             -- set when the sum was typed by hand
  note            TEXT,
  voided          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS doc_line_doc ON doc_line(doc_id, pos);

-- Money in (received from a customer) or out (paid to a supplier).
CREATE TABLE IF NOT EXISTS payment (
  id              INTEGER PRIMARY KEY,
  party_id        INTEGER NOT NULL REFERENCES party(id),
  direction       TEXT    NOT NULL,                    -- in | out
  amount          INTEGER NOT NULL,
  day             INTEGER NOT NULL,
  method          TEXT    NOT NULL DEFAULT 'cash',     -- cash | card | cheque | other
  ref             TEXT,
  note            TEXT,
  voided          INTEGER NOT NULL DEFAULT 0,
  void_reason     TEXT,
  created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS payment_party ON payment(party_id, day DESC);

-- A payment applied to a specific invoice. Whatever is left unallocated is
-- credit on the party.
CREATE TABLE IF NOT EXISTS allocation (
  id              INTEGER PRIMARY KEY,
  payment_id      INTEGER NOT NULL REFERENCES payment(id) ON DELETE CASCADE,
  doc_id          INTEGER NOT NULL REFERENCES doc(id),
  amount          INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS allocation_payment ON allocation(payment_id);
CREATE INDEX IF NOT EXISTS allocation_doc ON allocation(doc_id);

CREATE TABLE IF NOT EXISTS cheque (
  id              INTEGER PRIMARY KEY,
  party_id        INTEGER NOT NULL REFERENCES party(id),
  direction       TEXT    NOT NULL DEFAULT 'in',       -- in: I receive, out: I pay
  number          TEXT,
  bank            TEXT,
  amount          INTEGER NOT NULL,
  due_day         INTEGER NOT NULL,
  status          TEXT    NOT NULL DEFAULT 'pending',  -- pending | cleared | bounced | returned
  note            TEXT,
  payment_id      INTEGER REFERENCES payment(id),
  created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS cheque_due ON cheque(status, due_day);

CREATE TABLE IF NOT EXISTS task (
  id              INTEGER PRIMARY KEY,
  text            TEXT    NOT NULL,
  day             INTEGER,
  party_id        INTEGER REFERENCES party(id),
  done            INTEGER NOT NULL DEFAULT 0,
  created_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS note (
  id              INTEGER PRIMARY KEY,
  day             INTEGER NOT NULL,
  title           TEXT,
  body            TEXT    NOT NULL DEFAULT '',
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sum_sheet (
  id              INTEGER PRIMARY KEY,
  day             INTEGER NOT NULL,
  title           TEXT,
  created_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sum_row (
  id              INTEGER PRIMARY KEY,
  sheet_id        INTEGER NOT NULL REFERENCES sum_sheet(id) ON DELETE CASCADE,
  pos             INTEGER NOT NULL,
  label           TEXT    NOT NULL DEFAULT '',
  amount          INTEGER NOT NULL DEFAULT 0,
  party_id        INTEGER REFERENCES party(id),
  struck          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS sum_row_sheet ON sum_row(sheet_id, pos);

-- Nothing is ever deleted. Every write that changes a document, payment or
-- balance lands here first, which is the app's version of a crossed-out line.
CREATE TABLE IF NOT EXISTS audit (
  id              INTEGER PRIMARY KEY,
  entity          TEXT    NOT NULL,
  entity_id       INTEGER NOT NULL,
  action          TEXT    NOT NULL,                    -- create | update | void | settle
  payload         TEXT    NOT NULL DEFAULT '{}',
  at              INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS audit_entity ON audit(entity, entity_id, at DESC);

CREATE TABLE IF NOT EXISTS setting (
  key             TEXT PRIMARY KEY,
  value           TEXT NOT NULL
);
