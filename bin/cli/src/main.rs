use crate::command::*;
use clap::Clap;
use sunshine_cli_utils::Result;
use test_client::Client;

mod command;

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opts: Opts = Opts::parse();
    let root = if let Some(root) = opts.path {
        root
    } else {
        dirs::config_dir().unwrap().join("sunshine-bounty")
    };
    let mut client = match opts.chain_spec_path {
        Some(spec) => Client::new(&root, spec.as_path()).await?,
        None => Client::new(&root, "ws://127.0.0.1:9944").await?,
    };

    match opts.cmd {
        SubCommand::Key(KeyCommand { cmd }) => {
            match cmd {
                KeySubCommand::Set(cmd) => cmd.exec(&mut client).await?,
                KeySubCommand::Unlock(cmd) => cmd.exec(&mut client).await?,
                KeySubCommand::Lock(cmd) => cmd.exec(&mut client).await?,
            }
        }
        SubCommand::Wallet(WalletCommand { cmd }) => {
            match cmd {
                WalletSubCommand::GetAccountBalance(cmd) => {
                    cmd.exec(&client).await?
                }
                WalletSubCommand::TransferBalance(cmd) => {
                    cmd.exec(&client).await?
                }
            }
        }
        SubCommand::Org(OrgCommand { cmd }) => {
            match cmd {
                OrgSubCommand::IssueShares(cmd) => cmd.exec(&client).await?,
                OrgSubCommand::BurnShares(cmd) => cmd.exec(&client).await?,
                OrgSubCommand::BatchIssueShares(cmd) => {
                    cmd.exec(&client).await?
                }
                OrgSubCommand::BatchBurnShares(cmd) => {
                    cmd.exec(&client).await?
                }
                OrgSubCommand::RegisterFlatOrg(cmd) => {
                    cmd.exec(&client).await?
                }
                OrgSubCommand::RegisterWeightedOrg(cmd) => {
                    cmd.exec(&client).await?
                }
            }
        }
        SubCommand::Vote(VoteCommand { cmd }) => {
            match cmd {
                VoteSubCommand::CreateSignalThresholdVote(cmd) => {
                    cmd.exec(&client).await?
                }
                VoteSubCommand::CreatePercentThresholdVote(cmd) => {
                    cmd.exec(&client).await?
                }
                VoteSubCommand::SubmitVote(cmd) => cmd.exec(&client).await?,
            }
        }
        SubCommand::Donate(DonateCommand { cmd }) => {
            match cmd {
                DonateSubCommand::PropDonate(cmd) => cmd.exec(&client).await?,
                DonateSubCommand::EqualDonate(cmd) => cmd.exec(&client).await?,
            }
        }
        SubCommand::Bank(BankCommand { cmd }) => {
            match cmd {
                BankSubCommand::Open(cmd) => cmd.exec(&client).await?,
                BankSubCommand::ProposeSpend(cmd) => cmd.exec(&client).await?,
                BankSubCommand::TriggerVote(cmd) => cmd.exec(&client).await?,
                BankSubCommand::SudoApprove(cmd) => cmd.exec(&client).await?,
                BankSubCommand::Close(cmd) => cmd.exec(&client).await?,
            }
        }
        SubCommand::Bounty(BountyCommand { cmd }) => {
            match cmd {
                BountySubCommand::PostBounty(cmd) => cmd.exec(&client).await?,
                BountySubCommand::ContributeToBounty(cmd) => {
                    cmd.exec(&client).await?
                }
                BountySubCommand::SubmitForBounty(cmd) => {
                    cmd.exec(&client).await?
                }
                BountySubCommand::ApproveApplication(cmd) => {
                    cmd.exec(&client).await?
                }
                BountySubCommand::GetBounty(cmd) => cmd.exec(&client).await?,
                BountySubCommand::GetSubmission(cmd) => {
                    cmd.exec(&client).await?
                }
                BountySubCommand::GetOpenBounties(cmd) => {
                    cmd.exec(&client).await?
                }
                BountySubCommand::GetOpenSubmissions(cmd) => {
                    cmd.exec(&client).await?
                }
            }
        }
    }
    Ok(())
}
