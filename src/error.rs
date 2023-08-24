use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("NotEnoughNftsToClaim")]
    NotEnoughNftsToClaim {},

    #[error("InvalidDistributionEndTime")]
    InvalidDistributionEndTime {},

    #[error("CurrentDistributionHasNotYetEnded")]
    CurrentDistributionHasNotYetEnded {},

    #[error("NotImplementedYet")]
    NotImplementedYet {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("CurrentUnclaimedGoesToTheSubsequentDistribution")]
    CurrentUnclaimedGoesToTheSubsequentDistribution {},

    #[error("NothingToReturn")]
    NothingToReturn {},

    #[error("UnclaimedWasAlreadyReturned")]
    UnclaimedWasAlreadyReturned {},

    #[error("InvalidFundsReceived")]
    InvalidFundsReceived {},

    #[error("TooManyTokensSent")]
    TooManyTokensSent {},

    #[error("DistributionHalted")]
    DistributionHalted {},

    #[error("InvalidNftCount")]
    InvalidNftCount {},

    #[error("ClaimingWindowHasClosed")]
    ClaimingWindowHasClosed {},

    #[error("NothingToClaim")]
    NothingToClaim {},

    #[error("InvalidDistributionInputs")]
    InvalidDistributionInputs {},

    #[error("InvalidClaimValue")]
    InvalidClaimValue {},
}
