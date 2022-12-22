use std::{
	collections::{HashSet, VecDeque},
	net::SocketAddr,
	sync::{Arc, RwLock},
	time::Duration,
};

use axum::{
	extract::{
		ws::{Message, WebSocket, WebSocketUpgrade},
		ConnectInfo,
	},
	response::Response,
	Extension,
};

use futures::{sink::SinkExt, stream::StreamExt};
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::{
	sync::{broadcast, watch},
	time,
};

use crate::v1::db::{db_chunk::DBChunk, Access, ChunkId, ChunkVec, ChunkView, SortType, ViewType};

use super::{auth::UserClaims, ends::DB};

/**
 * Defines a Socket Message Type
 */
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum MessageType {
	// Id + Ok + Value?
	Ok,
	// Id + Err + Value?
	#[serde(rename = "Err")]
	Error,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct SocketMessage {
	#[serde(skip_serializing_if = "Option::is_none")]
	id: Option<usize>,
	#[serde(rename = "type", skip_serializing_if = "Option::is_none")]
	pub _type: Option<MessageType>,
	#[serde(skip_serializing_if = "String::is_empty")]
	pub resource: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<String>,
}
/**
 * (Value)
 */
impl<T: Serialize> From<&T> for SocketMessage {
	fn from(value: &T) -> Self {
		Self {
			value: serde_json::to_string(value).ok(),
			..Default::default()
		}
	}
}
/**
 * (Type, Value)
 */
impl<T: Serialize> From<(MessageType, &T)> for SocketMessage {
	fn from((_type, value): (MessageType, &T)) -> Self {
		Self {
			_type: Some(_type),
			value: serde_json::to_string(value).ok(),
			..Default::default()
		}
	}
}
/**
 * (Type)
 */
impl From<MessageType> for SocketMessage {
	fn from(_type: MessageType) -> Self {
		Self {
			_type: Some(_type),
			..Default::default()
		}
	}
}

#[derive(Clone, Debug)]
pub struct ResourceMessage {
	pub id: usize,
	// pub _type: MessageType,
	pub resource: String,
	pub value: Value,
	pub users: HashSet<String>,
}
impl Default for ResourceMessage {
	fn default() -> Self {
		Self {
			id: resource_id_next(),
			resource: Default::default(),
			value: Default::default(),
			users: Default::default(),
		}
	}
}
/**
 * (Resource, Users)
 */
impl From<(&str, HashSet<String>)> for ResourceMessage {
	fn from((resource, users): (&str, HashSet<String>)) -> Self {
		Self {
			resource: resource.into(),
			users,
			..Default::default()
		}
	}
}
/**
 * (Resource, Users, Value)
 */
impl<T: Serialize> From<(&str, HashSet<String>, &T)> for ResourceMessage {
	fn from((resource, users, value): (&str, HashSet<String>, &T)) -> Self {
		Self {
			resource: resource.into(),
			users,
			value: json!(value),
			..Default::default()
		}
	}
}

static mut RESOURCE_ID: usize = 0;
fn resource_id_next() -> usize {
	unsafe {
		let j = RESOURCE_ID.clone();
		RESOURCE_ID += 1;
		j
	}
}

// pub type ResourceReceiver = broadcast::Receiver<ResourceChange>;
pub type ResourceSender = broadcast::Sender<ResourceMessage>;

pub async fn websocket_handler(
	ws: WebSocketUpgrade,
	Extension(_user): Extension<UserClaims>,
	Extension(db): Extension<DB>,
	Extension(tx_r): Extension<ResourceSender>,
	Extension(shutdown_rx): Extension<watch::Receiver<()>>,
	ConnectInfo(address): ConnectInfo<SocketAddr>,
) -> Response {
	info!("Opening Websocket with {} on {}.", &_user.user, address);
	ws.on_upgrade(move |socket| handle_socket(socket, _user, db, tx_r, shutdown_rx, address))
}

async fn handle_socket(
	socket: WebSocket,
	user_claims: UserClaims,
	db: DB,
	tx_resource: ResourceSender,
	mut shutdown_rx: watch::Receiver<()>,
	address: SocketAddr,
) {
	let user = &user_claims.user;
	// Create a new receiver for our Broadcast
	let mut rx_resource = tx_resource.subscribe();

	let (mut tx_socket, mut rx_socket) = socket.split();

	let get_notes = || {
		if user == "public" {
			return json!([]);
		}

		let mut chunks: ChunkVec = db.write().unwrap().get_chunks(user).into();
		chunks.sort(SortType::Modified);
		let chunks = chunks.0.into_iter().map(|v| ChunkView::from((v, user.as_str(), ViewType::Notes))).collect::<Vec<_>>();
		json!(chunks)
	};

	/// [[parent,parent], [child,child]]
	let get_subtree = |root: Option<&str>, view_type: ViewType| {
		if user == "public" {
			return json!([[], []]);
		}
		let root = root.and_then(|id| db.read().unwrap().get_chunk(id, user));
		let subtree = 
			// Graph
			db.write().unwrap().subtree(
				root.as_ref(),
				&user.as_str().into(),
				&|v| {
					let mut vec = ChunkVec::from(v);
					vec.sort(SortType::ModifiedDynamic(user.as_str().into()));
					vec.into()
				},
				&|v| json!(ChunkView::from((v, user.as_str(), view_type))),
				1,
			)
		
		;
		json!(subtree)
	};

	// Keep last resource id so when we're sending
	// a message in resource stream, we don't process
	// the message on the instance that sent it
	// (if it was incremented by that instance beforehand)
	let resource_id_last = RwLock::new(0);

	let handle_incoming = |m| {
		if let Message::Text(m) = m {
			let m = serde_json::from_str::<SocketMessage>(&m).unwrap();
			let reply = |mut v: SocketMessage| {
				v.resource = m.resource.to_owned();
				v.id = m.id;
				// Send ok if id exists but message doesn't have any, and remove status if id doesn't exist
				match v.id {
					Some(_) => {
						if v._type.is_none() {
							v._type = Some(MessageType::Ok)
						}
					}
					None => {
						v._type = None;
					}
				}
				Some(Message::Text(serde_json::to_string(&v).unwrap()))
			};
			let mut res = m.resource.split("/").collect::<VecDeque<_>>();
			let mut piece = res.pop_front();

			if piece == Some("chunks") {
				if let Some(id) = res.pop_front() {
					// If an id was provided
					if let Some(value) = m.value {
						// User wants to change a value
						// if (m._type == MessageType::Change){return None;}
						let db_chunk: DBChunk = (id, value.as_str()).into();
						let users = db_chunk.access_users();
						match db.write().unwrap().update_chunk_with_diff(db_chunk, user) {
							Ok((users_to_notify, diff)) => {
								let m = ResourceMessage::from((format!("chunks/{}/diff", id).as_str(), users, &diff));
								{
									// Update our resource_id_last so we don't send the same data back when sending a signal to tx_resource
									let mut resource_id_last = resource_id_last.write().unwrap();
									*resource_id_last = m.id;
								}
								tx_resource.send(m).unwrap();

								if users_to_notify.len() > 0 {
									tx_resource
										.send(ResourceMessage::from(("chunks", users_to_notify)))
										.unwrap();
								}

								return reply(MessageType::Ok.into());
							}
							Err(err) => return reply((MessageType::Error, &format!("{err:?}")).into()),
						}
					} else {
						// Request for "chunks/<id>"
						if let Some(v) = db.read().unwrap().get_chunk(id, &user) {
							return reply((&ChunkView::from((v, user.as_str()))).into());
						}
					}
					return reply((MessageType::Error, &format!("NotFound")).into());
				} else {
					// Request for "chunks"
					return reply((&get_notes()).into());
				}
			} else if piece == Some("views") {
				piece = res.pop_front();
				let root_id = res.pop_front();
				if piece == Some("notes") {
					return reply((&get_notes()).into());
				} else if piece == Some("well") {
					return reply((&get_subtree(root_id, ViewType::Well)).into());
				} else if piece == Some("graph") {
					return reply((&get_subtree(root_id, ViewType::Graph)).into());
				}
				error!("View needs name");
				return None;
			} else if piece == Some("user") {
				let mut user = json!(&user_claims);
				if let Value::Object(mut user_o) = user {
					let mut db = db.write().unwrap();
					let chunks = db.get_chunks(&user_claims.user);
					user_o.insert("notes_visible".into(), chunks.iter().count().into());
					user_o.insert(
						"notes_owned".into(),
						chunks
							.iter()
							.filter(|chunk| chunk.read().unwrap().chunk().owner == user_claims.user)
							.count()
							.into(),
					);
					user_o.insert(
						"notes_owned_public".into(),
						chunks
							.iter()
							.filter(|chunk| {
								let chunk = chunk.read().unwrap();
								chunk.chunk().owner == user_claims.user && chunk.has_access(&"public".into())
							})
							.count()
							.into(),
					);
					user = json!(user_o);
				}
				return reply((&user).into());
			}

			error!("Message {m:?} unknown");
		}

		None
	};

	let handle_resource = |message: ResourceMessage| -> Vec<String> {
		let mut messages = vec![];
		{
			// Only continue if the message's id is greater than our last processed id
			let mut resource_id_last = resource_id_last.write().unwrap();
			if message.id <= *resource_id_last {
				return messages;
			}
			*resource_id_last = message.id;
		}
		// Only continue if the connected user is part of the list of users in the message
		if !message.users.contains(user) {
			return messages;
		}
		info!("Triggered '{}' to '{}'", &message.resource, user);

		let mut socket_message = SocketMessage::from(&message.value);
		socket_message.resource = message.resource.clone();
		messages.push(serde_json::to_string(&socket_message).unwrap());

		messages
	};
	loop {
		tokio::select! {
			// Handles Websocket incomming
			m = rx_socket.next() => {
				if let Some(m) = m{

					if let Ok(m) = m {
						// info!("Received {m:?}");
						if let Some(m) = handle_incoming(m){
							tx_socket.send(m).await.unwrap();
						};
					}else{
						info!("Received Err from {address}, client disconnected");
						break;
					}
				}else{
					info!("Received None from {address}, client disconnected");
					break;
				}
			}
			// Handles resource incoming
			m = rx_resource.recv() => {
				if let Ok(m) = m {
					let ms = handle_resource(m);
					for m in ms {
						tx_socket.feed(Message::Text(m)).await.unwrap();
					}
					if let Err(err) = tx_socket.flush().await {

							info!("Got {err:?} while sending to {address}, assuming client disconnected");
							break;

					};
				}else{
					error!("Received Err resource {m:?} on {address}, closing connection.");
					match tx_socket.close().await{
						Ok(()) => {info!("Socket {address} closed successfully!")}
						Err(err) => {error!("Got {err:?} on {address} while closing");}
					}
					break;
				}
			}
			_ = shutdown_rx.changed() => {
				break;
			}
			// Send a ping message
			_ = time::sleep(Duration::from_secs(20u64)) => {
				tx_socket.send(Message::Ping(vec![50u8])).await.unwrap();
				continue;
			}
		}
	}

	info!("Closed socket with {user} on {address}");
}
