# Frozen native-retained protocol references

`native-retained-golden.csv` contains 96 functional configurations from the old
native retained-tree opening path, before storage-policy removal. Both base fields,
QA/RS,r2/4/8,c2/4 and depths1–4 are covered. They are uncertified small test
configurations, not the100-bit experiment table.

`native-retained-provenance.json` records the original protocol source commit/file
hashes, seeds and generator/fixture hashes. The generator source is retained as an
inert fixture file; it is not another active performance entry. It was executed
against protocol source byte-identical to7bd0724 (unchanged from c587b84).

The current test exercises owning one-shot opening, not the original native tree
mode, and compares canonical commitment/proof/post-challenge fingerprints under
1/32 functional threads. Do not regenerate these references to accept an unintended
protocol/encoding change. Restore the frozen source to reproduce their provenance.
