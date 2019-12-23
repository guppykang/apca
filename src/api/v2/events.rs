// Copyright (C) 2019 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::SystemTime;

use num_decimal::Num;

use serde::Deserialize;

use crate::api::time_util::optional_system_time;
use crate::api::v2::account;
use crate::api::v2::order;
use crate::events::EventStream;
use crate::events::StreamType;


/// A representation of an account update that we receive through the
/// "account_updates" stream.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AccountUpdate {
  /// The corresponding account's ID.
  #[serde(rename = "id")]
  pub id: account::Id,
  /// The time the account was created at.
  #[serde(rename = "created_at", deserialize_with = "optional_system_time")]
  pub created_at: Option<SystemTime>,
  /// The time the account was updated last.
  #[serde(rename = "updated_at", deserialize_with = "optional_system_time")]
  pub updated_at: Option<SystemTime>,
  /// The time the account was deleted at.
  #[serde(rename = "deleted_at", deserialize_with = "optional_system_time")]
  pub deleted_at: Option<SystemTime>,
  /// The account's status.
  #[serde(rename = "status")]
  pub status: String,
  /// The currency the account uses.
  #[serde(rename = "currency")]
  pub currency: String,
  /// Cash balance.
  #[serde(rename = "cash")]
  pub cash: Num,
  /// Withdrawable cash amount.
  #[serde(rename = "cash_withdrawable")]
  pub withdrawable_cash: Num,
}


/// A type used for requesting a subscription to the "account_updates"
/// event stream.
#[derive(Clone, Copy, Debug)]
pub enum AccountUpdates {}

impl EventStream for AccountUpdates {
  type Event = AccountUpdate;

  fn stream() -> StreamType {
    StreamType::AccountUpdates
  }
}


/// The status of a trade, as reported as part of a `TradeUpdate`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum TradeStatus {
  /// The order has been received by Alpaca, and routed to exchanges for
  /// execution.
  #[serde(rename = "new")]
  New,
  /// The order has been partially filled.
  #[serde(rename = "partial_fill")]
  PartialFill,
  /// The order has been filled, and no further updates will occur for
  /// the order.
  #[serde(rename = "fill")]
  Filled,
  /// The order is done executing for the day, and will not receive
  /// further updates until the next trading day.
  #[serde(rename = "done_for_day")]
  DoneForDay,
  /// The order has been canceled, and no further updates will occur for
  /// the order.
  #[serde(rename = "canceled")]
  Canceled,
  /// The order has expired, and no further updates will occur.
  #[serde(rename = "expired")]
  Expired,
  /// The order is waiting to be canceled.
  #[serde(rename = "pending_cancel")]
  PendingCancel,
  /// The order has been stopped, and a trade is guaranteed for the
  /// order, usually at a stated price or better, but has not yet
  /// occurred.
  #[serde(rename = "stopped")]
  Stopped,
  /// The order has been rejected, and no further updates will occur for
  /// the order.
  #[serde(rename = "rejected")]
  Rejected,
  /// The order has been suspended, and is not eligible for trading.
  /// This state only occurs on rare occasions.
  #[serde(rename = "suspended")]
  Suspended,
  #[serde(rename = "pending_new")]
  /// The order has been received by Alpaca, and routed to the
  /// exchanges, but has not yet been accepted for execution.
  PendingNew,
  /// The order has been completed for the day (either filled or done
  /// for day), but remaining settlement calculations are still pending.
  #[serde(rename = "calculated")]
  Calculated,
}


/// A representation of a trade update that we receive through the
/// "trade_updates" stream.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TradeUpdate {
  /// The event that occurred.
  #[serde(rename = "event")]
  pub event: TradeStatus,
  /// The order associated with the trade.
  #[serde(rename = "order")]
  pub order: order::Order,
}

/// A type used for requesting a subscription to the "trade_updates"
/// event stream.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(unused)]
pub enum TradeUpdates {}

impl EventStream for TradeUpdates {
  type Event = TradeUpdate;

  fn stream() -> StreamType {
    StreamType::TradeUpdates
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use futures::future::ok;
  use futures::FutureExt;
  use futures::StreamExt;
  use futures::TryStreamExt;

  use test_env_log::test;

  use url::Url;

  use crate::api::API_BASE_URL;
  use crate::api::v1::order_util::order_aapl;
  use crate::api::v2::order;
  use crate::api_info::ApiInfo;
  use crate::Client;
  use crate::Error;


  #[test(tokio::test)]
  async fn stream_trade_events() -> Result<(), Error> {
    // TODO: There may be something amiss here. If we don't cancel the
    //       order we never get an event about a new trade. That does
    //       not seem to be in our code, though, as the behavior is the
    //       same when streaming events using Alpaca's Python client.
    let api_info = ApiInfo::from_env()?;
    let client = Client::new(api_info);
    let stream = client.subscribe::<TradeUpdates>().await?;
    let order = order_aapl(&client).await?;
    let _ = client.issue::<order::Delete>(order.id).await?;

    let trade = stream
      .try_filter_map(|res| {
        assert!(res.is_ok(), "error: {:?}", res.unwrap_err());
        ok(res.ok())
      })
      // There could be other trades happening concurrently but we
      // are only interested in ones belonging to the order we
      // submitted as part of this test.
      .try_skip_while(|trade| ok(trade.order.id != order.id))
      // In fact, we only care about the first such trade for our
      // verification purposes.
      .take(1)
      .into_future()
      // We don't care about the rest of the stream. Well, there
      // really shouldn't be any.
      .map(|(trade, _stream)| trade)
      .await
      .unwrap()?;

    assert_eq!(order.id, trade.order.id);
    assert_eq!(order.asset_id, trade.order.asset_id);
    assert_eq!(order.symbol, trade.order.symbol);
    assert_eq!(order.asset_class, trade.order.asset_class);
    assert_eq!(order.type_, trade.order.type_);
    assert_eq!(order.side, trade.order.side);
    assert_eq!(order.time_in_force, trade.order.time_in_force);
    Ok(())
  }

  #[test(tokio::test)]
  async fn stream_with_invalid_credentials() -> Result<(), Error> {
    let api_base = Url::parse(API_BASE_URL)?;
    let api_info = ApiInfo {
      base_url: api_base,
      key_id: b"invalid".to_vec(),
      secret: b"invalid-too".to_vec(),
    };

    let client = Client::new(api_info);
    let result = client.subscribe::<TradeUpdates>().await;

    match result {
      Ok(_) => panic!("operation succeeded unexpectedly"),
      Err(Error::Str(ref e)) if e == "authentication not successful" => (),
      Err(e) => panic!("received unexpected error: {}", e),
    }
    Ok(())
  }
}