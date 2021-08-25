# Buttcoin distributor
btn.group's per block Buttcoin distributor for YieldOptimizer.

## How it works
* Buttcoin is sent into smart contract.
* Admin sets the general per block release schedule.
* Admin sets the weight for each address that can claim.

## The three pillars of blockchain
The three pillars refers to blockchain itself but we are attempting to follow the ethos as much as possible.

### 1. Decentralization
This smart contract is centralized as the admin can set everything and relies on the admin to send Buttcoin into it.

### 2. Transparency
The viewing key for Buttcoin for this smart contract has been made public which means that it's fully auditable.

### 3. Immutability
Buttcoin follows the SNIP-20 standard.

## Regarding privacy
We have thought long and hard about this and have decided to make many aspects public. This means that it would be pretty easy for someone to calculate who deposited how much.

We thought about a centralized option where we only hold the viewing keys and show a delayed balance, but this would mean that the user base would have to take our word for it.

The point of blockchain is to be decentralized and trustless. One scam I can think of off the top of my head would be to add a smart contract but only expose that to ourselves. That way we can accrue Buttcoin for ourselves and dump on the market.

We think privacy is important, but it should be privacy for individuals and transparency for organizations.

## Testing locally
```
// 1. Run chain locally
docker run -it --rm -p 26657:26657 -p 26656:26656 -p 1337:1337 -v $(pwd):/root/code --name secretdev enigmampc/secret-network-sw-dev

// 2. Access container via separate terminal window
docker exec -it secretdev /bin/bash

// 3. cd into code folder
cd code

// 4. Store the contract (Specify your keyring. Mine is named test etc.)
secretcli tx compute store buttcoin-distributor.wasm.gz --from a --gas 3000000 -y --keyring-backend test

// 5. Get the contract's id
secretcli query compute list-code

// 6. Init Buttcoin 
CODE_ID=2
INIT='{"buttcoin": {"address": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg", "contract_hash": "4CD7F64B9ADE65200E595216265932A0C7689C4804BE7B4A5F8CEBED250BF7EA"}, "release_schedule": [{"end_block": 3333333333, "release_per_block": "1000000"}], "viewing_key": "testing"}'
secretcli tx compute instantiate $CODE_ID "$INIT" --from a --label "buttcoin-distributor" -y --keyring-backend test --gas 3000000 --gas-prices=3.0uscrt
```
