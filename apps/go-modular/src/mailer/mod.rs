//! lettre SMTP mailer + askama email templates.
//!
//! Scaffold placeholder. Real mailer lands during D-SMTP-1..4:
//! - Transport builder with port-based TLS selection (587 STARTTLS,
//!   465 implicit TLS, 25/1025 plaintext for dev only).
//! - askama-derived `EmailVerificationTemplate` rendering.
//! - `Mailer::send_email(to, subject, html_body)` with pool reuse.
