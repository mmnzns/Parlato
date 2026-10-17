// Parlato : telechargement HTTP partage par les gestionnaires de modeles
// (Whisper, GGUF, Parakeet, VAD). Chacun avait sa propre boucle, sans delai
// d'expiration ni reprise : une connexion bloquee figeait la barre de
// progression pour toujours.
//
// Reference VoiceInk : VoiceInk/Transcription/Core/Whisper/WhisperModelManager.swift
// (URLSession.downloadTask gere seul delais et fichier temporaire ; ici on le
// fait a la main avec reqwest).
//
// - Client : 15 s pour se connecter, 30 s sans recevoir un octet = erreur.
// - Fichier `.part` : garde apres un echec ou un blocage. La tentative
//   suivante envoie `Range: bytes=N-` ; reponse 206 = on reprend a N,
//   reponse 200 = le serveur renvoie tout, on repart de zero.
// - Annulation par l'utilisateur : le `.part` est supprime (Annuler veut
//   dire recommencer, comme avant).
// - Empreinte SHA-256 (si le catalogue en donne une) calculee au fil de
//   l'eau ; si elle ne correspond pas, le `.part` est supprime.
// - Fin : flush + sync, puis renommage du `.part` vers la cible, pour qu'un
//   fichier modele present soit toujours complet.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tracing::info;

/// Delai maximal pour etablir la connexion.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Duree maximale sans recevoir un octet avant d'abandonner.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Frequence de verification de l'annulation et du blocage pendant l'attente.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Message d'erreur d'une annulation. Le frontend le reconnait a ce code
/// (translateError.ts isCancelled) : ne pas le changer.
pub const CANCELLED: &str = "PARLA_ERR:cancelled";

/// Client HTTP des telechargements de modeles, avec les delais ci-dessus.
pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(STALL_TIMEOUT)
        .build()
        .context("creation du client HTTP")
}

/// Un fichier a telecharger.
pub struct Transfer<'a> {
    pub url: &'a str,
    /// Fichier final, cree seulement quand le telechargement est complet.
    pub target: &'a Path,
    /// Fichier partiel, garde entre deux tentatives pour la reprise.
    pub part: &'a Path,
    /// Drapeau d'annulation du gestionnaire (None = pas d'annulation).
    pub cancel: Option<&'a AtomicBool>,
    /// Intervalle minimal entre deux appels de `on_progress`.
    pub progress_every: Duration,
    /// Empreinte SHA-256 attendue (hex), None = pas de verification.
    pub sha256: Option<&'a str>,
}

impl Transfer<'_> {
    /// Telecharge `url` vers `target`. `on_progress(telecharge, total)` est
    /// appele au plus toutes les `progress_every`, puis une fois a la fin ;
    /// `telecharge` compte aussi les octets repris du `.part`. `total` vaut
    /// None si le serveur ne donne pas la taille. Retourne la taille finale.
    pub async fn run(
        &self,
        client: &reqwest::Client,
        on_progress: impl FnMut(u64, Option<u64>),
    ) -> Result<u64> {
        self.run_with_stall(client, STALL_TIMEOUT, on_progress)
            .await
    }

    async fn run_with_stall(
        &self,
        client: &reqwest::Client,
        stall_timeout: Duration,
        mut on_progress: impl FnMut(u64, Option<u64>),
    ) -> Result<u64> {
        let mut offset = tokio::fs::metadata(self.part)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let mut resp = self.send(client, offset).await?;
        let mut append = false;
        if offset > 0 {
            let status = resp.status();
            if status == StatusCode::PARTIAL_CONTENT && content_range_start(&resp) == Some(offset) {
                append = true;
                info!(url = self.url, offset, "Reprise du telechargement");
            } else if status == StatusCode::OK {
                // Le serveur ignore Range : il renvoie tout, on repart de zero.
                offset = 0;
            } else {
                // 416 (le .part est plus long que le fichier) ou plage
                // inattendue : on oublie le .part et on redemande tout.
                drop(resp);
                offset = 0;
                resp = self.send(client, 0).await?;
            }
        }
        if !resp.status().is_success() {
            bail!("HTTP {} depuis {}", resp.status(), self.url);
        }
        let total = resp.content_length().map(|len| offset + len);

        let mut file = if append {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(self.part)
                .await
        } else {
            tokio::fs::File::create(self.part).await
        }
        .with_context(|| format!("ouverture de {}", self.part.display()))?;

        // En reprise, l'empreinte commence par le contenu deja recu.
        let mut hasher = match self.sha256 {
            None => None,
            Some(_) if append => {
                let part = self.part.to_path_buf();
                Some(
                    tokio::task::spawn_blocking(move || hash_file(&part))
                        .await
                        .context("calcul de l'empreinte")??,
                )
            }
            Some(_) => Some(Sha256::new()),
        };

        let mut stream = resp.bytes_stream();
        let mut downloaded = offset;
        let mut last_byte = Instant::now();
        let mut last_emit = Instant::now();
        on_progress(downloaded, total);

        loop {
            if self.cancel.is_some_and(|c| c.load(Ordering::SeqCst)) {
                drop(file);
                let _ = std::fs::remove_file(self.part);
                return Err(anyhow!(CANCELLED));
            }
            match tokio::time::timeout(POLL_INTERVAL, stream.next()).await {
                Err(_) => {
                    if last_byte.elapsed() >= stall_timeout {
                        bail!(
                            "aucune donnee recue depuis {} s ({})",
                            stall_timeout.as_secs(),
                            self.url
                        );
                    }
                }
                Ok(None) => break,
                Ok(Some(chunk)) => {
                    let bytes = chunk.context("reception des donnees")?;
                    file.write_all(&bytes)
                        .await
                        .with_context(|| format!("ecriture de {}", self.part.display()))?;
                    if let Some(h) = hasher.as_mut() {
                        h.update(&bytes);
                    }
                    downloaded += bytes.len() as u64;
                    last_byte = Instant::now();
                    if last_emit.elapsed() >= self.progress_every {
                        on_progress(downloaded, total);
                        last_emit = Instant::now();
                    }
                }
            }
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);

        if let Some(total) = total {
            if downloaded != total {
                bail!("telechargement incomplet : {downloaded} octets sur {total}");
            }
        }
        if let (Some(expected), Some(h)) = (self.sha256, hasher) {
            if !digest_matches(h, expected) {
                // Fichier corrompu ou modifie : inutile de le reprendre.
                let _ = std::fs::remove_file(self.part);
                bail!(
                    "le fichier telecharge ne correspond pas a l'empreinte SHA-256                      attendue, il a ete supprime ({})",
                    self.url
                );
            }
        }
        on_progress(downloaded, total);

        std::fs::rename(self.part, self.target).with_context(|| {
            format!(
                "renommage {} -> {}",
                self.part.display(),
                self.target.display()
            )
        })?;
        Ok(downloaded)
    }

    async fn send(&self, client: &reqwest::Client, offset: u64) -> Result<reqwest::Response> {
        let mut req = client.get(self.url);
        if offset > 0 {
            req = req.header(RANGE, format!("bytes={offset}-"));
        }
        req.send()
            .await
            .with_context(|| format!("GET {}", self.url))
    }
}

/// Empreinte SHA-256 en cours du contenu de `path` (fichier entier).
fn hash_file(path: &Path) -> Result<Sha256> {
    use std::io::Read;
    let mut file =
        std::fs::File::open(path).with_context(|| format!("lecture de {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hasher);
        }
        hasher.update(&buf[..n]);
    }
}

/// Vrai si l'empreinte finale vaut `expected` (hex, casse indifferente).
fn digest_matches(hasher: Sha256, expected: &str) -> bool {
    format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected.trim())
}

/// Tests des catalogues : l'URL Hugging Face pointe sur un commit (40 hex,
/// pas "main") et l'empreinte est un SHA-256 hex.
#[cfg(test)]
pub fn assert_pinned(url: &str, sha256: &str) {
    let is_hex = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
    let revision = url
        .split("/resolve/")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    assert!(is_hex(revision, 40), "URL non figee sur un commit : {url}");
    assert!(is_hex(sha256, 64), "SHA-256 invalide pour {url}");
}

/// Premier octet annonce par `Content-Range: bytes N-M/T`.
fn content_range_start(resp: &reqwest::Response) -> Option<u64> {
    let value = resp.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    let range = value.trim().strip_prefix("bytes")?.trim_start();
    range.split('-').next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    /// Comportement du mini serveur HTTP de test.
    #[derive(Clone, Copy)]
    enum Mode {
        /// Accepte `Range` et repond 206.
        Ranges,
        /// Ignore `Range` et renvoie tout en 200.
        NoRanges,
        /// Envoie quelques octets puis se tait.
        Stall,
    }

    fn body() -> Vec<u8> {
        (0..10_000u32).map(|i| (i % 251) as u8).collect()
    }

    /// Serveur HTTP/1.1 minimal sur 127.0.0.1. Retourne l'URL et la liste
    /// des en-tetes Range recus (une entree par requete).
    async fn serve(mode: Mode) -> (String, Arc<parking_lot::Mutex<Vec<Option<String>>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let ranges = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let seen = ranges.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let seen = seen.clone();
                tokio::spawn(async move {
                    let mut req = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                        let n = sock.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        req.extend_from_slice(&buf[..n]);
                    }
                    let text = String::from_utf8_lossy(&req).to_string();
                    let range = text
                        .lines()
                        .find_map(|l| l.strip_prefix("range: ").or(l.strip_prefix("Range: ")))
                        .map(|s| s.trim().to_string());
                    seen.lock().push(range.clone());

                    let data = body();
                    let start = match (mode, &range) {
                        (Mode::Ranges, Some(r)) => r
                            .trim_start_matches("bytes=")
                            .trim_end_matches('-')
                            .parse::<usize>()
                            .unwrap(),
                        _ => 0,
                    };
                    let head = if start > 0 {
                        format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                            data.len() - start,
                            start,
                            data.len() - 1,
                            data.len()
                        )
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            data.len()
                        )
                    };
                    let _ = sock.write_all(head.as_bytes()).await;
                    if let Mode::Stall = mode {
                        let _ = sock.write_all(&data[..10]).await;
                        tokio::time::sleep(Duration::from_secs(20)).await;
                        return;
                    }
                    let _ = sock.write_all(&data[start..]).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        (format!("http://{addr}/model.bin"), ranges)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "parlato-download-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    async fn fetch(
        url: &str,
        dir: &Path,
        cancel: Option<&AtomicBool>,
        stall: Duration,
        sha256: Option<&str>,
    ) -> Result<u64> {
        let target = dir.join("model.bin");
        let part = dir.join("model.bin.part");
        Transfer {
            url,
            target: &target,
            part: &part,
            cancel,
            progress_every: Duration::from_millis(50),
            sha256,
        }
        .run_with_stall(&client().unwrap(), stall, |_, _| {})
        .await
    }

    #[tokio::test]
    async fn downloads_whole_file() {
        let (url, ranges) = serve(Mode::Ranges).await;
        let dir = temp_dir("whole");
        let n = fetch(&url, &dir, None, STALL_TIMEOUT, None).await.unwrap();
        assert_eq!(n, body().len() as u64);
        assert_eq!(std::fs::read(dir.join("model.bin")).unwrap(), body());
        assert!(!dir.join("model.bin.part").exists());
        assert_eq!(ranges.lock().as_slice(), &[None]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn resumes_from_part_file() {
        let (url, ranges) = serve(Mode::Ranges).await;
        let dir = temp_dir("resume");
        std::fs::write(dir.join("model.bin.part"), &body()[..4000]).unwrap();
        fetch(&url, &dir, None, STALL_TIMEOUT, None).await.unwrap();
        assert_eq!(std::fs::read(dir.join("model.bin")).unwrap(), body());
        assert_eq!(ranges.lock().as_slice(), &[Some("bytes=4000-".to_string())]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn restarts_when_server_ignores_range() {
        let (url, _) = serve(Mode::NoRanges).await;
        let dir = temp_dir("norange");
        std::fs::write(dir.join("model.bin.part"), b"garbage").unwrap();
        fetch(&url, &dir, None, STALL_TIMEOUT, None).await.unwrap();
        assert_eq!(std::fs::read(dir.join("model.bin")).unwrap(), body());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn stall_fails_and_keeps_part_file() {
        let (url, _) = serve(Mode::Stall).await;
        let dir = temp_dir("stall");
        let err = fetch(&url, &dir, None, Duration::from_secs(1), None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("aucune donnee"), "{err}");
        assert!(!dir.join("model.bin").exists());
        assert_eq!(
            std::fs::read(dir.join("model.bin.part")).unwrap(),
            &body()[..10]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn cancel_deletes_part_file() {
        let (url, _) = serve(Mode::Stall).await;
        let dir = temp_dir("cancel");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            flag.store(true, Ordering::SeqCst);
        });
        let err = fetch(&url, &dir, Some(&cancel), STALL_TIMEOUT, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("PARLA_ERR:cancelled"), "{err}");
        assert!(!dir.join("model.bin.part").exists());
        assert!(!dir.join("model.bin").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn body_sha256() -> String {
        format!("{:x}", Sha256::digest(body()))
    }

    #[test]
    fn hash_file_matches_known_digest() {
        let dir = temp_dir("hash");
        let path = dir.join("abc.txt");
        std::fs::write(&path, b"abc").unwrap();
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(digest_matches(hash_file(&path).unwrap(), expected));
        assert!(digest_matches(
            hash_file(&path).unwrap(),
            &expected.to_uppercase()
        ));
        assert!(!digest_matches(hash_file(&path).unwrap(), &"0".repeat(64)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn checksum_ok_after_resume() {
        let (url, _) = serve(Mode::Ranges).await;
        let dir = temp_dir("sha-resume");
        std::fs::write(dir.join("model.bin.part"), &body()[..4000]).unwrap();
        let sha = body_sha256();
        fetch(&url, &dir, None, STALL_TIMEOUT, Some(&sha))
            .await
            .unwrap();
        assert_eq!(std::fs::read(dir.join("model.bin")).unwrap(), body());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn checksum_mismatch_deletes_file() {
        let (url, _) = serve(Mode::Ranges).await;
        let dir = temp_dir("sha-bad");
        let err = fetch(&url, &dir, None, STALL_TIMEOUT, Some(&"0".repeat(64)))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("SHA-256"), "{err}");
        assert!(!dir.join("model.bin").exists());
        assert!(!dir.join("model.bin.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Telechargement reel du modele VAD (moins de 1 Mo) depuis Hugging Face,
    /// puis reprise a partir d'un `.part` tronque (redirection vers le CDN
    /// comprise). Reseau requis :
    /// `cargo test --lib hf_download_and_resume_smoke -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn hf_download_and_resume_smoke() {
        let url = crate::transcription::vad::VAD_MODEL_URL;
        let dir = temp_dir("hf");
        let sha = Some(crate::transcription::vad::VAD_MODEL_SHA256);
        let n = fetch(url, &dir, None, STALL_TIMEOUT, sha).await.unwrap();
        let full = std::fs::read(dir.join("model.bin")).unwrap();
        assert_eq!(n, full.len() as u64);

        std::fs::remove_file(dir.join("model.bin")).unwrap();
        std::fs::write(dir.join("model.bin.part"), &full[..full.len() / 2]).unwrap();
        let mut first = None;
        let target = dir.join("model.bin");
        let part = dir.join("model.bin.part");
        Transfer {
            url,
            target: &target,
            part: &part,
            cancel: None,
            progress_every: Duration::from_millis(50),
            sha256: sha,
        }
        .run(&client().unwrap(), |done, _| {
            first.get_or_insert(done);
        })
        .await
        .unwrap();
        assert_eq!(first, Some(full.len() as u64 / 2), "pas de reprise");
        assert_eq!(std::fs::read(&target).unwrap(), full);
        println!("{n} octets, reprise a {} OK", full.len() / 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
