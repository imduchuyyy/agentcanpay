# Recorded API responses

Trimmed captures of real upstream responses, so the test suite can check
decoding without depending on a third party being reachable.

Re-record after an upstream shape change:

```sh
curl -s "https://public-backend.socket.tech/v3/swap/supported-chains" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); d['result']=d['result'][:3]; \
      json.dump(d, open('supported_chains.json','w'), indent=1)"

curl -s "https://public-backend.socket.tech/v3/swap/tokens/list\
?userAddress=0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045&chainIds=1,8453&list=trending" \
  > token_list_full.json   # then trim to a few tokens per chain
```

`0xd8dA…6045` is vitalik.eth, used because it holds enough tokens for the
fixture to contain non-zero balances.
