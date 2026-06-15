// src/plugins/mod.rs
//
// Application plugin extension points.
//
// The first supported plugin category is payment. More categories can be added
// later, for example storage, transcoding, notifications, and job queues.

pub mod payment;
pub mod storage;
