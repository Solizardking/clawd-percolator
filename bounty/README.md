# Bounty work

Audits of toly's **percolator-lean** bounty program, imported into this fork so
the whole body of Percolator work lives in one tree.

## percolator-lean/

The complete findings record of the 70 USDC percolator-lean bug bounty
(rounds 1 and 2), imported 2026-11-26 from the private findings repo
`Solizardking/percolator-bounty` @ `591675b` on the operator's order.

- `WRITEUP.md` — **C-1**: close/revive type confusion forging ~8.5e18
  insurance shares. Round-1 pot claimed on mainnet (+70 USDC). Filed publicly
  as [aeyakovenko/percolator-cli#106](https://github.com/aeyakovenko/percolator-cli/issues/106)
  (2026-11-26); the maintainer fix at percolator-lean head `16ed3fc` closes
  C-1 and the supporting leads.
- `reports/C-1-exploit-and-leads.md` — the exploit write-up and lead list.
- `track-a.md` … `track-c.md`, `findings/track-d.md` … `findings/track-g.md` —
  the per-track audit reports.
- `ROUND2.md` + `findings/track-g.md` — round 2: the pot's self-drain was
  traced to the program's own bad-debt waterfall against an insolvent house
  portfolio — accounting moving through intended instructions, not an
  exploit. Whether it counts as a claim is the maintainer's call.
- `lead-001-refreshleg-forcehaircut-DRAFT.md` — the Lead-001 draft (resolved
  upstream in the C-1 fix commits: RefreshLeg is now owner-signed).
- `poc/` — the claim/proof scripts. They load the claimer keypair from an
  external path at runtime; **no key material is in this tree** (secrets scan
  clean at import).
- `evidence/` — program ELFs, disassembly, and program-data dumps the
  verdicts rest on.
