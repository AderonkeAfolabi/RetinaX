# Platform Governance

## Overview

This platform implements decentralized governance using:

- Governance token with voting power
- Proposal creation and voting
- Execution timelock for approved proposals
- Delegation support

## Process

1. **Propose**: Any user above the proposal threshold can submit a proposal with an associated category.
2. **Vote**: Token holders vote for or against within the voting period.
3. **Quorum & Thresholds**: Proposals evaluate quorum and pass threshold according to their configured category (`ProposalCategory`).
4. **Timelock & Veto Window**: Approved proposals enter timelock queue. An emergency veto guardian (`guardian`) can cancel malicious proposals during the timelock window.
5. **Execution**: Approved proposals are executed via the timelock contract after the delay expires.
6. **Delegation**: Users can delegate voting power to others.

## Proposal Categories

Proposals can belong to distinct categories configured with unique quorum and pass threshold parameters:
- `ParameterChange`: Numeric or operational adjustments (e.g. 15% quorum, 51% threshold).
- `TreasurySpend`: Capital distributions and grants (e.g. 10% quorum, 51% threshold).
- `ContractUpgrade`: Smart contract WASM migrations (e.g. 30% quorum, 51% threshold).
- `General`: Access-control and general policies (e.g. 20% quorum, 51% threshold).
- `Emergency`: Fast-path urgent actions (e.g. 5% quorum, 51% threshold).

## Emergency Veto Guardian

A designated guardian role configured on the Governor and Timelock contracts can immediately cancel/veto a proposal while it is queued in the Timelock window, protecting against governance attacks before execution occurs.

## Smart Contracts

- `contracts/governor` — governance logic (proposals, voting, queueing)
- `contracts/timelock` — timelock execution delay
