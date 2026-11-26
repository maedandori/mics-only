//! Conservative pacing for the single lease holder. Measured duration includes
//! connect/auth/query/close. Never increase concurrency or modify Firebird.
use std::time::{Duration,Instant};
pub(crate) fn due(last:Option<Instant>,seconds:u64)->bool{
 last.is_none_or(|at|at.elapsed()>=Duration::from_secs(seconds))
}
pub(crate) struct AdaptivePolling{fast:u64,slow:u64,current:u64,healthy:u8}
impl AdaptivePolling{
 pub(crate) fn new(fast:u64,configured:u64)->Self{
  let fast=fast.clamp(5,20);
  Self{fast,slow:configured.clamp(30,60),current:fast,healthy:0}
 }
 pub(crate) fn production_seconds(&self)->u64{self.current}
 pub(crate) fn schedule_seconds(&self,normal:u64)->u64{normal.max(self.current)}
 fn target(&self,elapsed:Duration,ok:bool)->u64{
  if !ok{return 60}
  match elapsed.as_millis(){
   0..=299=>self.fast,
   300..=499=>10.max(self.fast),
   500..=999=>20.max(self.fast),
   _=>self.slow,
  }
 }
 pub(crate) fn observe_load(&mut self,elapsed:Duration,ok:bool)->bool{
  let target=self.target(elapsed,ok);
  if target>self.current{self.current=target;self.healthy=0;return true}
  if target>=self.current&&target>self.fast{self.healthy=0;}
  false
 }
 pub(crate) fn observe(&mut self,elapsed:Duration,ok:bool)->bool{
  if self.observe_load(elapsed,ok){return true}
  let target=self.target(elapsed,ok);
  if target<self.current&&ok{
   self.healthy=self.healthy.saturating_add(1);
   if self.healthy>=5{
    self.current=target.max(match self.current{31..=60=>30,21..=30=>20,11..=20=>10,_=>self.fast});
    self.healthy=0;return true
   }
  }else{self.healthy=0}
  false
 }
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn load_increases_immediately_and_recovers_gradually(){
  let mut p=AdaptivePolling::new(5,30);
  assert_eq!(p.production_seconds(),5);assert_eq!(p.schedule_seconds(20),20);
  assert!(p.observe(Duration::from_millis(350),true));assert_eq!(p.production_seconds(),10);
  assert!(p.observe(Duration::from_millis(800),true));assert_eq!(p.production_seconds(),20);
  assert!(p.observe(Duration::from_millis(1200),true));assert_eq!(p.production_seconds(),30);
  assert!(p.observe(Duration::from_millis(50),false));assert_eq!(p.production_seconds(),60);
  assert_eq!(p.schedule_seconds(20),60);
  for expected in [30,20,10,5]{
   for _ in 0..4{assert!(!p.observe(Duration::from_millis(80),true));}
   assert!(p.observe(Duration::from_millis(80),true));assert_eq!(p.production_seconds(),expected);
  }
 }
 #[test] fn independent_deadlines_and_schedule_cannot_accelerate_recovery(){
  let mut p=AdaptivePolling::new(5,30);
  let at=Instant::now()-Duration::from_secs(6);
  assert!(due(Some(at),p.production_seconds()));assert!(!due(Some(at),p.schedule_seconds(20)));
  p.observe_load(Duration::from_secs(2),true);
  for _ in 0..20{p.observe_load(Duration::from_millis(20),true);}
  assert_eq!(p.production_seconds(),30);
 }
}
