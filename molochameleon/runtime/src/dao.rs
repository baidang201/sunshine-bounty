// Copyright 2019 Amar Singh
// This file is part of MoloChameleon, licensed with the MIT License

#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(feature = "std")]
use runtime_primitives::traits::{Zero, As, Bounded}; // redefined Hash type below to make Rust compiler happy
use parity_codec::{HasCompact, Encode, Decode};
use support::{StorageValue, StorageMap, Parameter, Dispatchable, IsSubType, EnumerableStorageMap, dispatch::Result};
use support::{decl_module, decl_storage, decl_event, ensure};
use support::traits::{Currency, LockableCurrency}; 					// left out OnUnbalanced, WithdrawReason, LockIdentifier
use system::ensure_signed;
use rstd::ops::{Mul, Div}; // Add, Rem
use serde_derive::{Serialize, Deserialize};
use balances;

/// type aliasing for compilation
// type Hash = primitives::H256; // decided to just use `Vec<u8>` from Codec::encode
type AccountId = u64;

pub trait Trait: system::Trait {
	// the staking balance
	type Currency: LockableCurrency<Self::AccountId, Moment=Self::BlockNumber>;

	// overarching event type
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

// Sometimes I use this and sometimes I just use T::Currency ¯\_(ツ)_/¯
type BalanceOf<T> = <<T as Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;

// Wrapper around AccountId with permissioned withdrawal function (for ragequit)
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
#[derive(Encode, Decode, Clone, PartialEq, Eq)]
struct Pool<AccountId, Balance> {
	// The account for which the total balance is locked
	pub account: AccountId,
	// Total Shares
	pub shares: u32,
	// Total Balance
	pub funds: Balance,
}

/// Used as a hash in the main Proposal struct
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
#[derive(Encode, Decode, Clone, PartialEq, Eq)]
pub struct Base<AccountId, Balance> {
	proposer: AccountId,
	applicant: AccountId,
	sharesRequested: u32,
	tokenTribute: Balance,
}

/// A proposal to lock up tokens in exchange for shares
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, Debug))]
#[derive(Encode, Decode, Clone, PartialEq, Eq)]
pub struct Proposal<Hash, AccountId, Balance, BlockNumber> {
	base_hash: Hash,				 // hash of the proposal
	proposer: AccountId,			 // proposer AccountId (must be a member)
	applicant: AccountId,			 // applicant AccountId
	shares: u32, 					 // number of requested shares
	startTime: BlockNumber,			 // when the voting period starts
	graceStart: Option<BlockNumber>, // when the grace period starts (None if not started)
	yesVotes: u32,					 // number of shares that voted yes
	noVotes: u32,					 // number of shares that voted no
	maxVotes: u32,					 // used to check the number of shares necessary to pass
	processed: bool,				 // if processed, true
	passed: bool,					 // if passed, true
	tokenTribute: Balance, 	 	 	 // tokenTribute; optional (set to 0 if no tokenTribute)
}

decl_event!(
	pub enum Event<T> where Balance = BalanceOf<T>, <T as system::Trait>::AccountId 
	{
		Proposed(Vec<u8>, Balance, AccountId, AccountId),	// (proposal, tokenTribute, proposer, applicant)
		Aborted(Vec<u8>, Balance, AccountId, AccountId),	// (proposal, proposer, applicant)
		Voted(Hash, bool, u32, u32),		// (proposal, vote, yesVotes, noVotes)
		Processed(Vec<u8>, Balance, AccountId, bool),		// (proposal, tokenTribute, NewMember, executed_correctly)
		Withdrawal(AccountId, u32, Balance),		// => successful "ragequit" (member, shares, Balances)
	}
);

decl_storage! {
	trait Store for Module<T: Trait> as Dao {
		// The length of a voting period in sessions
		pub VotingPeriod get(voting_period) config(): T::BlockNumber = T::BlockNumber::sa(500);
		// the time after the voting period starts during which the proposer can abort
		pub AbortWindow get(abort_window) config(): T::BlockNumber = T::BlockNumber::sa(200);
		// The length of a grace period in sessions
		pub GracePeriod get(grace_period) config(): T::BlockNumber = T::BlockNumber::sa(1000);
		/// The current era index.
		pub CurrentEra get(current_era) config(): T::BlockNumber;

		pub ProposalBond get(proposal_bond) config(): BalanceOf<T>;
		pub DilutionBound get(dilution_bound) config(): u32;

		/// TRACKING PROPOSALS
		// Proposals that have been made (impl of `ProposalQueue`)
		pub Proposals get(proposals): map Vec<u8> => Proposal<T::AccountId, BalanceOf<T>, T::Hash, T::BlockNumber>;
		// Active Applicants (to prevent multiple applications at once)
		pub Applicants get(applicants): map T::AccountId => Vec<u8>; // may need to change to &T::AccountId

		/// VOTING
		// map: proposalHash => Voters that have voted (prevent duplicate votes from the same member)
		pub VoterId get(voter_id): map Vec<u8> => Vec<T::AccountId>;
		// map: proposalHash => yesVoters (these voters are locked in from ragequitting during the grace period)
		pub VotersFor get(voters_for): map Vec<u8> => Vec<T::AccountId>;
		// inverse of the function above for `remove_proposal` function
		pub ProposalsFor get(proposals_for): map T::AccountId => Vec<Vec<u8>>;
		// get the vote of a specific voter (simplify testing for existence of vote via `VoteOf::exists`)
		pub VoteOf get(vote_of): map (Vec<u8>, T::AccountId) => bool;

		/// Dao MEMBERSHIP - permanent state (always relevant, changes only at the finalisation of voting)
		pub MemberCount get(member_count) config(): u32; // the number of current DAO members
		pub ActiveMembers get(active_members) config(): Vec<T::AccountId>; // the current Dao members
		pub MemberShares get(member_shares): map T::AccountId => u32; // shares of the current Dao members

		/// INTERNAL ACCOUNTING
		// add total stake?
		// Address for the pool
		pub PoolAdress get(pool_address) config(): Pool<T::AccountId, BalanceOf<T>>;
		// Number of shares across all members
		pub TotalShares get(total_shares) config(): u32; 
		// total shares that have been requested in unprocessed proposals
		pub TotalSharesRequested get(total_shares_requested): u32; 
	}
	add_extra_genesis { // see `mock.rs::ExtBuilder::build` for usage
		config(members): Vec<T::AccountId, u32>; // (accountid, sharesOwned)
		config(applicants): Vec<T::AccountId, u32, BalanceOf<T>>; // (accountId, sharesRequested, tokenTribute)
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;

		fn propose(origin, applicant: AccountId, shares: u32, tokenTribute: BalanceOf<T>) -> Result {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_member(&who), "proposer is not a member of Dao");

			// check that too many shares aren't requsted ( 100% is a high upper bound)
			ensure!(shares <= Self::total_shares(), "too many shares requested");

			// check that applicant doesn't have a pending application
			ensure!(!<Applicants>::exists(&applicant), "applicant has pending application");

			// reserve member's bond for proposal
			T::Currency::reserve(&who, Self::proposal_bond())
				.map_err(|_| "balance of proposer is too low")?;

			// reserve applicant's tokenTribute for proposal
			T::Currency::reserve(&applicant, tokenTribute)
				.map_err(|_| "balance of applicant is too low")?;

			let yesVotes = <MemberShares<T>>::get(&who);
			let noVotes = 0u32;
			let maxVotes = Self::total_shares();
			Self::total_shares.set(maxVotes + shares);

			let base = Base {
				proposer: &who,
				applicant: &applicant,
				shares: shares,
				tokenTribute: tokenTribute,
			};

			// add proposal hash
			let hash = base.encode(); // the output of Codec::encode does the job
			// verify uniqueness of this hash
			ensure!(!<Proposals<T>>::exists(hash), "Key collision ;-(");

			let prop = Proposal {
				base_hash: hash,
				proposer: &who,
				applicant: &applicant,
				shares: shares,
				startTime: <system::Module<T>>::block_number(),
				graceStart: None,
				yesVotes: yesVotes,
				noVotes: noVotes,
				maxVotes: maxVotes,
				processed: false,
				passed: false,
				tokenTribute: tokenTribute,
			}

			<Proposals<T>>::insert(hash, prop);
			//add applicant
			<Applicants<T>>::insert(&applicant, hash);

			// add yes vote from member who sponsored proposal (and initiate the voting)
			<VotersFor<T>>::mutate(hash, |voters| voters.push(&who));
			// supporting map for `remove_proposal`
			<ProposalsFor<T>>::mutate(&who, |props| props.push(hash));
			// set that this account has voted
			<VoterId<T>>::mutate(hash, |voters| voters.push(&who));
			// set this for maintainability of other functions
			<VoteOf<T>>::insert(&(hash, who), true);

			Self::deposit_event(RawEvent::Proposed(hash, tokenTribute, &who, &applicant));
			Self::deposit_event(RawEvent::Voted(hash, true, yesVotes, noVotes));

			Ok(())
		}
		
		/// Allow revocation of the proposal without penalty within the abortWindow
		fn abort(origin, hash: Vec<u8>) -> Result {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_member(&who), "proposer is not a member of Dao");

			// check if proposal exists
			ensure!(<Proposals<T>>::exists(hash), "proposal does not exist");

			let proposal = <Proposals<T>>::get(hash);

			ensure!(proposal.proposer == &who, "Only the proposer can abort");

			// check that the abort is within the window
			ensure!(
				proposal.startTime + Self::abort_window() >= <system::Module<T>>::block_number(),
				"it is past the abort window"
			);

			// return the proposalBond back to the proposer because they aborted
			T::Currency::unreserve(&proposal.proposer, Self::proposal_bond());
			// and the tokenTribute to the applicant
			T::Currency::unreserve(&proposal.applicant,
			proposal.tokenTribute);

			proposal.aborted = true;

			Self::remove_proposal(hash)?;

			Self::deposit_event(RawEvent::Aborted(hash, proposal.tokenTribute, proposal.proposer, proposal.applicant));

			Ok(())
		}

		fn vote(origin, hash: Vec<u8>, approve: bool) {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_member(&who), "proposer is not a member of Dao");
			
			ensure!(<Proposals<T>>::exists(hash), "proposal does not exist");

			// load proposal
			let proposal = <Proposals<T>>::get(hash);

			ensure!(
				(proposal.startTime + <VotingPeriod<T>>::get() >= <system::Module<T>>::block_number()) 
				&& !proposal.passed, 
				format!("The voting period has passed with yes: {} no: {}", proposal.yesVotes, proposal.noVotes)
			);

			// check that member has not yet voted
			ensure!(<VoteOf<T>>::exists(hash, &who), "voter has already submitted a vote on this proposal");

			// FIX unncessary conditional path
			if approve {
				<VotersFor<T>>::mutate(hash, |voters| voters.push(&who));
				<ProposalsFor<T>>::insert(&who, |props| props.push(hash));
				<VoterId<T>>::mutate(hash, |voters| voters.push(&who));
				<VoteOf<T>>::insert(&(hash, who), true);

				// to bound dilution for yes votes
				if <TotalShares<T>>::get() > proposal.maxVotes {
					proposal.maxVotes = <TotalShares<T>>::get();
				}
				proposal.yesVotes += Self::member_shares(&who);

			} else {
				proposal.noVotes += Self::member_shares(&who);
			}

			// proposal passes => switch to the grace period (during which nonsupporters (who have no pending proposals) can exit)
			if proposal.majority_passed() {
				proposal.graceStart = <system::Module<T>>::block_number();
				proposal.passed = true;
			}

			Self::deposit_event(RawEvent::Voted(hash, approve, proposal.yesVotes, proposal.noVotes));

			Ok(())
		}

		fn process(origin, hash: Vec<u8>) -> Result {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_member(&who), "proposer is not a member of the DAO");
			ensure!(<Proposals<T>>::exists(hash), "proposal does not exist");

			let proposal = <Proposals<T>>::get(hash);

			// if dilution bound not satisfied, wait until there are more shares before passing
			ensure!(
				<TotalShares<T>>::get().checked_mul(<DilutionBound<T>>::get()) > proposal.maxVotes, 
				"Dilution bound not satisfied; wait until more shares to pass vote"
			);
			
			let grace_period = (
				(proposal.graceStart <= <system::Module<T>>::block_number()) 
				&& (<system::Module<T>>::block_number() < proposal.graceStart + <GracePeriod<T>>::get())
			);
			let status = proposal.passed;

			if (!grace_period && status) {
				// transfer the proposalBond back to the proposer
				T::Currency::unreserve(&proposal.proposer, Self::proposal_bond());
				// transfer 50% of the proposal bond to the processer
				T::Currency::transfer(&proposal.proposer, &who, Self::proposal_bond().checked_mul(0.5));
				// return the applicant's tokenTribute
				T::Currency::unreserve(&proposal.applicant,
				proposal.tokenTribute);

				Self::remove_proposal(hash);

				let late_time = <system::Module<T>>::block_number - proposal.graceStart + <GracePeriod<T>>::get();

				Self::deposit_event(RawEvent::RemoveStale(hash, late_time));
			} else if (grace_period && status) {
				// transfer the proposalBond back to the proposer because they aborted
				T::Currency::unreserve(&proposal.proposer, Self::proposal_bond());
				// and the applicant's tokenTribute
				T::Currency::unreserve(&proposal.applicant,
				proposal.tokenTribute);

				// HARDCODED PROCESSING REWARD (todo: make more flexible)
				// transaction fee for proposer and processer comes from tokenTribute
				let txfee = proposal.tokenTribute.checked_mul(0.05); // check if this works (underflow risk?)
				let _ = T::Currency::make_transfer(&proposal.applicant, &who, txfee);
				let _ = T::Currency::make_transfer(&proposal.applicant, &proposal.proposer, txfee);

				let netTribute = proposal.tokenTribute * 0.9;

				// transfer tokenTribute to Pool
				let poolAddr = Self::pool_address();
				let _ = T::Currency::make_transfer(&proposal.applicant, &poolAddr, netTribute);

				// mint new shares
				Self::total_shares.mutate(|total| total += proposal.shares);

				// if applicant is already a member, add to their existing shares
				if proposal.applicant.is_member() {
					<MemberShares<T>>::mutate(proposal.applicant, |shares| shares += proposal.shares);
				} else {
					// if applicant is a new member, create a new record for them
					<MemberShares<T>>::insert(proposal.applicant, proposal.shares);
				}
			} else {
				Err("The proposal did not pass")
			}
			
			Self::remove_proposal(hash);
		
			Self::deposit_event(RawEvent::Processed(hash, status));

			Ok(())
		}

		fn rage_quit(origin, sharesToBurn: u32) -> Result {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_member(&who), "proposer is not a member of the DAO");

			let shares = <MemberShares<T>>::get(&who);
			ensure!(shares >= sharesToBurn, "insufficient shares");

			// check that all proposals have passed
			ensure!(<ProposalsFor<T>>::get(&who).iter()
					.all(|prop| prop.passed && 
						(<system::Module<T>>::block_number() <= prop.graceStart + Self::grace_period())
					), "All proposals have not passed or exited the grace period"
			);

			Self::pool_address().withdraw(&who, sharesToBurn);

			Ok(())
		}
	}
}

impl<AccountId, Balance, Hash, BlockNumber> Proposal<AccountId, Balance, Hash, BlockNumber> {
	// more than half shares voted yes
	pub fn majority_passed(&self) -> bool {
		// do I need the `checked_div` flag?
		if self.maxVotes % 2 == 0 { 
			return (self.yesVotes > self.maxVotes.checked_div(2)) 
		} else { 
			return (self.yesVotes > (self.maxVotes.checked_add(1).checked_div(2)))
		};
	}
}

impl<AccountId,
	Balance: HasCompact + Copy
> Pool<AccountId, Balance> {
	pub fn withdraw(&self, receiver: AccountId, sharedBurned: u32) -> Result { 
		// Checks on identity made in `rage_quit` (the only place in which this is called)

		let amount = Currency::free_balance(&self.account).checked_mul(sharedBurned).checked_div(Self::total_shares());
		let _ = Currency::make_transfer(&self.account, &receiver, amount)?;

		Self::deposit_event(RawEvent::Withdrawal(receiver, amount));

		Ok(())
	}
}

impl<T: Trait> Module<T> {
	pub fn is_member(who: &T::AccountId) -> Result {
		Self::active_members().iter()
			.any(|&(ref a, _)| a == who)?;
		Ok(())
	}

	// Clean up storage maps involving proposals
	pub fn remove_proposal(hash: Vec<u8>) -> Result {
		ensure!(<Proposals<T>>::exists(hash), "the given proposal does not exist");
		let proposal = <Proposals<T>>::get(hash);
		<Proposals<T>>::remove(hash);
		<Applicants<T>>::remove(proposal.applicant);

		let voters = <VotersFor<T>>::get(hash).iter().map(|voter| {
			<ProposalsFor<T>>::mutate(&voter, |hashes| hashes.iter().filter(|hush| hush != hash).collect());
			voter
		});

		<VoterId<T>>::remove(hash);
		<VotersFor<T>>::remove(hash);
		// reduce outstanding share request amount
		<TotalSharesRequested<T>>::set(|total| total -= proposal.shares);

		Ok(())
	} 
}