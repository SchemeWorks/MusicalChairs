//! Host-side reference model for the accepted native SOL economic contract.
//! No instruction entrypoint, wallet access or deployed program is provided.
#![forbid(unsafe_code)]
pub mod admission;
pub mod backers;
pub mod earnings;
pub mod fees;
pub mod rounds;
