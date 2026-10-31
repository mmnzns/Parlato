// Module transcription - pipeline Whisper local, Parakeet, VAD et cloud ;
// les moteurs locaux sont ceux de voice-engine-stt (speech.rs).
//
// Reference VoiceInk :
// - Transcription/Core/VoiceInkEngine.swift (orchestration)
// - Transcription/Core/TranscriptionPipeline.swift (etapes apres capture)
// - Transcription/Core/Whisper/LibWhisper.swift (params whisper.cpp)
// - Transcription/Core/Whisper/WhisperModelManager.swift (download + storage)
// - Models/PredefinedModels.swift (catalogue de modeles)

pub mod audio_file;
pub mod cloud;
pub mod engine;
pub mod error_code;
pub mod langs;
pub mod parakeet;
pub mod parakeet_windows;
pub mod pipeline;
pub mod speech;
pub mod vad;
pub mod whisper;
