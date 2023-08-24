# cw-nft-reward-distribution

Contract to distribute a fixed amount of coins for each NFT held by a wallet.

## Details
The distribution window's timer is decided when a distribution is added. When the timer has ended and some
users did not claim, the contract keeps the coins and will add them uniformly to the subsequent distribution.