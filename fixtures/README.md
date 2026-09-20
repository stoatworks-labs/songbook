# Fixtures

Files the vendors' own offline editors wrote, used by the byte-exact tests.

- `ql5/pfql_base.CLF` — QL Editor V5.8.1.27, a fresh QL5 file. `pfql_mod2` = CH4 → DANTE1;
  `pfql_mod3` = CH4 → DANTE1 and CH12 → DANTE2; `pfql_name` = CH8 renamed `ZZTOP`. The
  CLF writer must reproduce the three from the base byte-for-byte
  (`songbook-yamaha`, `clf::write_tests`).
- `sq7/NVDATA.DAT`, `sq7/SCENE001.DAT` — SQ MixPad 1.6.0 running offline as an SQ-7, its
  `CurrentShow` with Ip3 patched to Local 10 and scene 2 named "Scene 2". The SQ reader and the
  CRC-32 check run against them (`songbook-ah`, `sq::write_tests`).

Nothing here came from a console; see the README's Status for what that means.
