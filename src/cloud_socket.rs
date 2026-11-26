//! Persistent, TLS-authenticated, request/ACK transport. Never retry an
//! uncertain write over HTTP in the same call; existing sync retries later.
use std::{cell::RefCell,net::{TcpStream,ToSocketAddrs},time::{Duration,Instant}};
use serde_json::{json,Value};
use tungstenite::{client::IntoClientRequest,stream::MaybeTlsStream,Message,WebSocket};
type Socket=WebSocket<MaybeTlsStream<TcpStream>>;
struct Channel{
 endpoint:String,agent_id:String,coordinator:String,sync:String,
 socket:Option<Socket>,next_connect:Instant,backoff:u64,next_id:u64,
}
thread_local!{static CHANNEL:RefCell<Option<Channel>>=const{RefCell::new(None)};}
pub fn configure(c:&super::Config,coordinator:&str,sync:&str){
 let endpoint=format!("{}/socket",c.coordinator_url.trim_end_matches('/'));
 let Ok(mut u)=url::Url::parse(&endpoint) else{return};
 if u.scheme()!="https"||u.host_str().is_none()||u.username()!=""||u.password().is_some(){return}
 if u.set_scheme("wss").is_err(){return}
 CHANNEL.with(|cell|*cell.borrow_mut()=Some(Channel{endpoint:u.to_string(),agent_id:c.agent_id.clone(),coordinator:coordinator.to_owned(),sync:sync.to_owned(),socket:None,next_connect:Instant::now(),backoff:5,next_id:0}));
}
impl Channel{
 fn connect(&self)->Result<Socket,String>{
  let url=url::Url::parse(&self.endpoint).map_err(|_|"Invalid socket URL")?;
  let host=url.host_str().ok_or("Socket host missing")?;
  let port=url.port_or_known_default().unwrap_or(443);
  let addresses=(host,port).to_socket_addrs().map_err(|_|"Socket DNS failed")?;
  let mut tcp=None;
  for addr in addresses.take(2){
   if let Ok(stream)=TcpStream::connect_timeout(&addr,Duration::from_secs(3)){tcp=Some(stream);break}
  }
  let tcp=tcp.ok_or("Socket connection failed")?;
  tcp.set_read_timeout(Some(Duration::from_secs(6))).map_err(|_|"Socket read timeout configuration failed")?;
  tcp.set_write_timeout(Some(Duration::from_secs(6))).map_err(|_|"Socket write timeout configuration failed")?;
  let mut request=self.endpoint.as_str().into_client_request().map_err(|_|"Socket request invalid")?;
  let headers=request.headers_mut();
  headers.insert("authorization",format!("Bearer {}",self.coordinator).parse().map_err(|_|"Invalid coordinator credential")?);
  headers.insert("x-mics-sync-token",self.sync.parse().map_err(|_|"Invalid sync credential")?);
  headers.insert("x-mics-agent-id",self.agent_id.parse().map_err(|_|"Invalid agent ID")?);
  tungstenite::client_tls_with_config(request,tcp,None,None).map(|(ws,_)|ws).map_err(|_|"Socket handshake unavailable".into())
 }
 fn failed(&mut self){
  self.socket=None;self.next_connect=Instant::now()+Duration::from_secs(self.backoff);
  self.backoff=(self.backoff*2).min(60);
 }
 fn exchange(&mut self,op:&str,body:&Value,lease:Option<&super::Lease>)->Option<Result<Value,String>>{
  if self.socket.is_none(){
   if Instant::now()<self.next_connect{return None}
   match self.connect(){Ok(ws)=>{self.socket=Some(ws);self.backoff=5;},Err(_)=>{self.failed();return None}}
  }
  self.next_id=self.next_id.wrapping_add(1).max(1);
  let id=self.next_id;
  let mut msg=json!({"id":id,"op":op,"body":body});
  if let Some(lease)=lease{msg["epoch"]=json!(lease.epoch);msg["lease_token"]=json!(lease.lease_token);}
  let result=(||->Result<(u64,Value),String>{
   let ws=self.socket.as_mut().ok_or("Socket unavailable")?;
   ws.send(Message::Text(msg.to_string())).map_err(|_|"Socket send failed")?;
   let deadline=Instant::now()+Duration::from_secs(8);
   while Instant::now()<deadline{
    match ws.read().map_err(|_|"Socket response failed")?{
     Message::Text(text)=>{
      let response:Value=serde_json::from_str(&text).map_err(|_|"Invalid socket response")?;
      if response["id"].as_u64()!=Some(id){return Err("Socket ACK ID mismatch".into())}
      return Ok((response["status"].as_u64().ok_or("Socket ACK status missing")?,response["data"].clone()))
     },
     Message::Ping(_)=>{ws.flush().map_err(|_|"Socket pong failed")?;},
     Message::Close(_)=>return Err("Socket closed".into()),
     _=>{}
    }
   }
   Err("Socket response deadline expired".into())
  })();
  match result{
   Ok((status,data)) if (200..300).contains(&status)=>Some(Ok(data)),
   Ok((status,_))=>Some(Err(format!("MICS_SOCKET_HTTP_{status}"))),
   Err(e)=>{self.failed();Some(Err(e))}
  }
 }
}
pub fn call(op:&str,body:&Value,lease:Option<&super::Lease>)->Option<Result<Value,String>>{
 CHANNEL.with(|cell|cell.borrow_mut().as_mut().and_then(|channel|channel.exchange(op,body,lease)))
}
