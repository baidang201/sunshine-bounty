use crate::error::{
    Error,
    Result,
};
use clap::Clap;
use core::fmt::{
    Debug,
    Display,
};
use substrate_subxt::{
    sp_core::crypto::Ss58Codec,
    system::System,
    Runtime,
};
use sunshine_bounty_client::{
    bank::{
        Bank,
        BankClient,
    },
    org::Org,
};
use sunshine_core::Ss58;

#[derive(Clone, Debug, Clap)]
pub struct BankOpenOrgAccountCommand {
    pub seed: u128,
    pub hosting_org: u64,
    pub bank_operator: Option<String>,
}

impl BankOpenOrgAccountCommand {
    pub async fn exec<R: Runtime + Bank, C: BankClient<R>>(
        &self,
        client: &C,
    ) -> Result<(), C::Error>
    where
        <R as System>::AccountId: Ss58Codec,
        <R as Org>::OrgId: From<u64> + Display,
        <R as Bank>::Currency: From<u128> + Display,
    {
        let bank_operator = if let Some(acc) = &self.bank_operator {
            let new_acc: Ss58<R> = acc.parse()?;
            Some(new_acc.0)
        } else {
            None
        };
        let event = client
            .open_org_bank_account(
                self.seed.into(),
                self.hosting_org.into(),
                bank_operator,
            )
            .await
            .map_err(Error::Client)?;
        println!(
            "Account {} initialized new bank account {:?} with balance {} for Org {} with bank operator {:?}",
            event.seeder, event.new_bank_id, event.seed, event.hosting_org, event.bank_operator
        );
        Ok(())
    }
}
