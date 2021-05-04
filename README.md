# Incentivized equals reward compounder

## When to use:
1. Staking the reward token only (e.g. SEFI staking)

## Algorithm:
1. Contract deposits reward into farm contract
2. Contract claims new reward
3. Repeat

## Verifying build

Given the address of a contract, you can query its code hash (sha256) by running:
```
secretcli q compute contract-hash <contract-address>
```

You can verify that this hash is correct by comparing it to the decompressed
contract binary.

To get the contract binary for a specific tag or commit and calculate its hash,
run:
```
git checkout <tag-or-commit>
make compile-optimized-reproducible
gunzip -c contract.wasm.gz >contract.wasm
sha256sum contract.wasm
```

Now compare the result with the hash returned by `secretcli`.
If you compiled the same code that was used to build the deployed binary,
they should match :)

## References

- https://github.com/enigmampc/scrt-finance-rewards/tree/master/contracts/lp-staking
- https://github.com/valuedefi/vsafe-contracts
- https://github.com/enigmampc/snip20-reference-impl
- https://github.com/SecretFoundation/SNIPs/blob/master/SNIP-20.md
- https://github.com/enigmampc/SecretSwap/blob/master/contracts/secretswap_token/README.md
