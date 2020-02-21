//! Hyper transport implementation.

use crate::core::data::{
    message::{Message, Type},
    request, response,
    timetoken::Timetoken,
};
use crate::core::json;
use crate::core::{Transport, TransportService};
use async_trait::async_trait;
use log::debug;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use pubnub_util::encoded_channels_list::EncodedChannelsList;

#[async_trait]
impl TransportService<request::Publish> for Hyper {
    type Response = response::Publish;
    type Error = error::Error;

    async fn call(&self, request: request::Publish) -> Result<Self::Response, Self::Error> {
        // Prepare encoded message and channel.
        encode_json!(request.payload => encoded_payload);
        let encoded_channel = utf8_percent_encode(&request.channel, NON_ALPHANUMERIC);

        // Prepare the URL.
        let path_and_query = format!(
            "/publish/{pub_key}/{sub_key}/0/{channel}/0/{message}",
            pub_key = self.publish_key,
            sub_key = self.subscribe_key,
            channel = encoded_channel,
            message = encoded_payload,
        );
        let url = self.build_uri(&path_and_query)?;

        // Send network request.
        let response = self.http_client.get(url).await?;
        let data_json = handle_json_response(response).await?;

        // Parse timetoken.
        let timetoken = Timetoken {
            t: data_json[2].as_str().unwrap().parse().unwrap(),
            r: 0, // TODO
        };

        Ok(timetoken)
    }
}

#[async_trait]
impl TransportService<request::Subscribe> for Hyper {
    type Response = response::Subscribe;
    type Error = error::Error;

    async fn call(&self, request: request::Subscribe) -> Result<Self::Response, Self::Error> {
        // TODO: add caching of repeating params to avoid reencoding.

        // Prepare encoded channels and channel_groups.
        let encoded_channels = EncodedChannelsList::from(request.channels);
        let encoded_channel_groups = EncodedChannelsList::from(request.channel_groups);

        // Prepare the URL.
        let path_and_query = format!(
            "/v2/subscribe/{sub_key}/{channels}/0?channel-group={channel_groups}&tt={tt}&tr={tr}",
            sub_key = self.subscribe_key,
            channels = encoded_channels,
            channel_groups = encoded_channel_groups,
            tt = request.timetoken.t,
            tr = request.timetoken.r,
        );
        let url = self.build_uri(&path_and_query)?;

        // Send network request.
        let response = self.http_client.get(url).await?;
        let data_json = handle_json_response(response).await?;

        // Parse timetoken.
        let timetoken = Timetoken {
            t: data_json["t"]["t"].as_str().unwrap().parse().unwrap(),
            r: data_json["t"]["r"].as_u32().unwrap_or(0),
        };

        // Parse messages.
        let messages = data_json["m"]
            .members()
            .map(|message| Message {
                message_type: Type::from_json(&message["e"]),
                route: message["b"].as_str().map(str::to_string),
                channel: message["c"].to_string(),
                json: message["d"].clone(),
                metadata: message["u"].clone(),
                timetoken: Timetoken {
                    t: message["p"]["t"].as_str().unwrap().parse().unwrap(),
                    r: message["p"]["r"].as_u32().unwrap_or(0),
                },
                client: message["i"].as_str().map(str::to_string),
                subscribe_key: message["k"].to_string(),
                flags: message["f"].as_u32().unwrap_or(0),
            })
            .collect::<Vec<_>>();

        Ok((messages, timetoken))
    }
}