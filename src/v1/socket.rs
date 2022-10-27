use std::{collections::HashSet, sync::RwLock, time::Duration};

use axum::{
	extract::ws::{Message, WebSocket, WebSocketUpgrade},
	response::Response,
	Error, Extension,
};
use axum_extra::routing::Resource;
use futures::{
	pin_mut,
	sink::SinkExt,
	stream::{SplitSink, SplitStream, StreamExt},
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use tokio::{
	sync::{broadcast, watch},
	time,
};

use super::{auth::UserClaims, ends::DB};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub enum MessageType {
	// A request from client
	#[serde(rename = "Req")]
	#[default]
	Request,
	// A response to a request
	#[serde(rename = "Ok")]
	Ok,
	// An error occourred, usually because of a request
	#[serde(rename = "Err")]
	Error,
	// A change happened
	#[serde(rename = "C")]
	Change,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct SocketMessage {
	// #[serde(skip)]
	id: Option<u32>,
	#[serde(rename = "type")]
	pub _type: MessageType,
	pub resource: String,
	pub value: Option<String>,
}
#[derive(Clone, Debug)]
pub struct ResourceMessage {
	pub id: u32,
	pub resource: String,
	pub value: Option<String>,
	pub users: HashSet<String>,
}
impl ResourceMessage {
	pub fn new<T: Serialize>(resource: String, value: Option<&T>, users: HashSet<String>) -> Self {
		Self {
			id: unsafe {
				let j = RESOURCE_ID.clone();
				RESOURCE_ID += 1;
				j
			},
			resource,
			value: value.and_then(|value| Some(serde_json::to_string(value).unwrap())),
			users,
		}
	}
}
static mut RESOURCE_ID: u32 = 0;

// pub type ResourceReceiver = broadcast::Receiver<ResourceChange>;
pub type ResourceSender = broadcast::Sender<ResourceMessage>;

pub async fn websocket_handler(
	ws: WebSocketUpgrade,
	Extension(_user): Extension<UserClaims>,
	Extension(db): Extension<DB>,
	Extension(tx_r): Extension<ResourceSender>,
	Extension(shutdown_rx): Extension<watch::Receiver<()>>,
) -> Response {
	ws.on_upgrade(|socket| handle_socket(socket, _user, db, tx_r, shutdown_rx))
}

async fn handle_socket(
	mut socket: WebSocket,
	user_claims: UserClaims,
	db: DB,
	tx_resource: ResourceSender,
	mut shutdown_rx: watch::Receiver<()>,
) {
	let user = &user_claims.user;
	// Create a new receiver for our Broadcast
	let mut rx_resource = tx_resource.subscribe();

	let (mut tx_socket, mut rx_socket) = socket.split();

	info!("Websocket with {} opened.", user);

	let get_notes_ids = || {
		let mut chunks = db.read().unwrap().get_notes(user);
		chunks.sort_by_key(|(chunk, _)| -(chunk.modified as i128));
		let chunks = chunks.iter().map(|v| v.0.id.clone()).collect::<Vec<_>>();
		serde_json::to_string(&chunks).unwrap()
	};
	let get_chunk =
		|id| serde_json::to_string(&db.read().unwrap().get_chunk(Some(user.to_owned()), &id).unwrap()).unwrap();
	let get_well_ids = |root| {
		let mut chunks = db.read().unwrap().get_chunks(user.to_owned(), root, None).unwrap();
		chunks.0.sort_by_key(|t| -(t.0.modified as i128));
		let chunks = (chunks.0.iter().map(|v| v.0.id.clone()).collect::<Vec<_>>(), chunks.1);
		serde_json::to_string(&chunks).unwrap()
	};

	let resource_id_last = RwLock::new(0u32);
	// let socket_id_last = RwLock::new(0u32);
	let handle_incoming = |m| {
		match m {
			Message::Text(m) => {
				let m = serde_json::from_str::<SocketMessage>(&m).unwrap();
				let reply = |value, _type| {
					Message::Text(
						serde_json::to_string(&SocketMessage {
							id: m.id,
							resource: m.resource.clone(),
							value,
							_type,
						})
						.unwrap(),
					)
				};
				let res = m.resource.split("/").collect::<Vec<_>>();

				// let
				if res[0] == "chunks" {
					if res.len() > 1 {
						match m.value {
							// Updating chunk/:id
							Some(v) => {
								// if (m._type == MessageType::Change){return None;}
								let id = res[1];
								match db.write().unwrap().set_chunk(user, (Some(id.into()), v)) {
									Ok((chunk, users, users_access_changed)) => {
										// Send a resource message to all open sockets
										let m = ResourceMessage::new(m.resource.clone(), Some(&chunk), users);
										{
											// Update our resource_id_last so we don't send the same data back when sending a signal to tx_resource
											let mut resource_id_last = resource_id_last.write().unwrap();
											*resource_id_last = m.id;
										}
										tx_resource.send(m).unwrap();
										tx_resource
											.send(ResourceMessage::new::<()>(
												format!("chunks"),
												None,
												users_access_changed,
											))
											.unwrap();
										Some(reply(None, MessageType::Ok))
									}
									// Couldn't write, so reply with an Error
									Err(err) => Some(reply(Some(format!("{err:?}")), MessageType::Error)),
								}
							}
							// Requesting chunk/:id
							None => Some(reply(Some(get_chunk(res[1].into())), MessageType::Ok)),
						}
					} else {
						match m.value {
							// Is updating resource
							Some(v) => None,
							// Is requesting resource
							None => {
								let db = db.read().unwrap();
								Some(reply(Some(get_notes_ids()), MessageType::Ok))
							}
						}
					}
				} else if res[0] == "views" {
					if res.len() > 1 {
						if res[1] == "well" {
							Some(reply(
								Some(get_well_ids(if res.len() > 2 { Some(res[2].into()) } else { None })),
								MessageType::Ok,
							))
						} else {
							None
						}
					} else {
						error!("View needs name");
						None
					}
				} else {
					error!("Resource {} unknown", res[0]);
					None
				}
			}
			_ => None,
		}
	};

	let handle_resource = |m: ResourceMessage| -> Vec<String> {
		let mut ms = vec![];
		{
			// Only continue if the message's id is greater than our last processed id
			let mut resource_id_last = resource_id_last.write().unwrap();
			if m.id <= *resource_id_last {
				return ms;
			}
			*resource_id_last = m.id;
		}
		// Only continue if the connected user is part of the list of users in the message
		if !m.users.contains(user) {
			return ms;
		}
		info!("Triggered '{}' to '{}'", &m.resource, user);

		// Get the socket id, and increment it by 1
		// let socket_id = {let id = socket_id_last.write().unwrap();let _id = *id;*id+=1;_id};
		let mut push_m = |r, v| {
			let m = SocketMessage {
				id: None,
				resource: r,
				value: v,
				_type: MessageType::Change,
			};
			ms.push(serde_json::to_string(&m).unwrap());
		};
		push_m(m.resource, m.value);
		ms
	};


	loop {
		tokio::select! {
			// Handles Websocket incomming
			m = rx_socket.next() => {
				if let Some(m) = m{
					if let Ok(m) = m {
						info!("Received {m:?}");
						if let Some(m) = handle_incoming(m){
							tx_socket.send(m).await.unwrap();
						};
					}else{
						error!("{m:?}");
						break;
					}
				}else{
					error!("{m:?}");
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
					tx_socket.flush().await.unwrap();
				}else{
					error!("{m:?}");
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
	// info!("Reuniting socket and closing");
	let socket = tx_socket.reunite(rx_socket).unwrap();
	if let Err(err) = socket.close().await {
		error!("Closing socket failed {:?} with {}", err, user);
	} else {
		info!("Closed socket with {}", user)
	}

	// If we want to split it
	// tokio::spawn(write(sender));
	// tokio::spawn(read(receiver));
}

// async fn read(receiver: SplitStream<WebSocket>) {
// 	// ...
// }

// async fn write(sender: SplitSink<WebSocket, Message>) {
// 	// ...
// }
