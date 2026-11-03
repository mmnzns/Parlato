# Third-party models and notices

Parlato is licensed under GPL-3.0 (see `LICENSE`). It does not include, bundle
or host any speech or language model. When a user chooses a model, Parlato
downloads it directly from the host listed below (Hugging Face), onto the
user's own PC. Each model is licensed by its authors under the terms below,
and the user's use of it is governed by those terms.

Online services (OpenAI, Anthropic, Groq, Deepgram and the others offered in
Speech model and AI cleanup) are used with the user's own API key under that
provider's own terms. No model files are involved.

## Speech models (on this PC)

### NVIDIA Parakeet Unified EN 0.6B

Licensed by NVIDIA Corporation under the NVIDIA Open Model License.

- Model: https://huggingface.co/nvidia/parakeet-unified-en-0.6b
- License: https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/
  (copy in `licenses/NVIDIA-Open-Model-License.txt`)
- Use must be consistent with NVIDIA's Trustworthy AI terms:
  https://www.nvidia.com/en-us/agreements/trustworthy-ai/terms/
- ONNX conversion by bobNight, downloaded from
  https://huggingface.co/bobNight/parakeet-unified-en-0.6b-onnx at the fixed
  revision `09e9060322d99c5f070010724786e6ee090fd51d`. That repository is
  labelled CC-BY-4.0; as a conversion of NVIDIA's model it remains subject to
  the NVIDIA Open Model License.

### NVIDIA Nemotron 3.5 ASR Streaming 0.6B

Provided by NVIDIA Corporation under the OpenMDW License Agreement,
version 1.1 (OpenMDW-1.1).

- Model: https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b
- License: https://openmdw.ai/license/1-1/ (copy in `licenses/OpenMDW-1.1.txt`)
- ONNX export by altunenes, the author of parakeet-rs, downloaded from the
  `nemotron-3.5-asr-streaming-0.6b-onnx` folder of
  https://huggingface.co/altunenes/parakeet-rs at the fixed revision
  `4d2a8bc71f5c896ec40faa59732e6716295edaf2`. As an export of NVIDIA's model
  it remains subject to OpenMDW-1.1.

### Moondream Parakeet Ultra

Post-trained version of NVIDIA's parakeet-tdt-0.6b-v3 by Moondream, licensed
under CC-BY-4.0 (https://creativecommons.org/licenses/by/4.0/).

- Model: https://huggingface.co/moondream/parakeet-ultra
- Based on: https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3 (Copyright
  NVIDIA Corporation, CC-BY-4.0)
- ONNX conversion and int8 quantization by Olicorne, downloaded from
  https://huggingface.co/Olicorne/parakeet-tdt-0.6b-v3-ultra-onnx at the fixed
  revision `3646b2f1a516a2310810902bfaa67515f3c89d38` (CC-BY-4.0)

### NVIDIA Parakeet TDT 0.6B v2 and v3

Copyright NVIDIA Corporation, licensed under CC-BY-4.0
(https://creativecommons.org/licenses/by/4.0/).

- Models: https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2,
  https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3
- ONNX conversions by istupakov:
  https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx,
  https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx (CC-BY-4.0)

### OpenAI Whisper

Whisper models by OpenAI (https://github.com/openai/whisper), released under
the MIT License (large-v3 is published on Hugging Face under Apache-2.0).
GGML conversions by Georgi Gerganov, https://huggingface.co/ggerganov/whisper.cpp
(MIT), downloaded at the fixed revision
`5359861c739e955e79d9a303bcbc70fb988958b1`.

### Silero VAD (voice detection)

Silero VAD by the Silero Team (https://github.com/snakers4/silero-vad),
released under the MIT License. GGML conversion by ggml-org,
https://huggingface.co/ggml-org/whisper-vad (MIT), downloaded at the fixed
revision `9ffd54a1e1ee413ddf265af9913beaf518d1639b`.

## AI cleanup models (on this PC)

### Qwen3.5 4B and 9B

Qwen3.5 by Alibaba Cloud (Qwen team), licensed under the Apache License 2.0
(https://www.apache.org/licenses/LICENSE-2.0). GGUF conversions by Unsloth,
downloaded at fixed revisions:

- unsloth/Qwen3.5-4B-GGUF `e87f176479d0855a907a41277aca2f8ee7a09523`
- unsloth/Qwen3.5-9B-GGUF `3885219b6810b007914f3a7950a8d1b469d598a5`

### Models no longer offered

Qwen 2.5 3B, Llama 3.2 3B, Gemma 2 2B and Phi-3.5 mini are no longer offered
for download. Files people downloaded earlier keep working as imported models,
so their notices stay below.

| Model | Author | License |
|---|---|---|
| Granite 4.2 3B | IBM | Apache-2.0, https://huggingface.co/ibm-granite/granite-4.2-3b |
| Qwen 2.5 3B Instruct | Alibaba Cloud | Qwen Research License, https://huggingface.co/Qwen/Qwen2.5-3B-Instruct/blob/main/LICENSE |
| Llama 3.2 3B Instruct | Meta | Llama 3.2 Community License, https://www.llama.com/llama3_2/license/ |
| Gemma 2 2B Instruct | Google | Gemma Terms of Use, https://ai.google.dev/gemma/terms |
| Phi 3.5 Mini Instruct | Microsoft | MIT, https://huggingface.co/microsoft/Phi-3.5-mini-instruct/resolve/main/LICENSE |

Required notices:

- Qwen is licensed under the Qwen RESEARCH LICENSE AGREEMENT, Copyright (c)
  Alibaba Cloud. All Rights Reserved. The Qwen Research License permits
  non-commercial use only (research or evaluation). For that reason Parlato
  no longer offers Qwen 2.5 3B for download (since 2026-09-29); it stays
  usable only for people who downloaded it earlier. Granite 4.2 3B
  (Apache-2.0) replaces it.
- Built with Llama. Llama 3.2 is licensed under the Llama 3.2 Community
  License, Copyright (c) Meta Platforms, Inc. All Rights Reserved.

GGUF conversions of the Qwen, Llama, Gemma and Phi models by bartowski
(https://huggingface.co/bartowski); Granite GGUF published by IBM. Each file
is downloaded at a fixed revision of its repository:

- ibm-granite/granite-4.2-3b-GGUF `c40945d71cd90f249a56985e8155551a9188dc30`
- bartowski/Qwen2.5-3B-Instruct-GGUF `f302c64a2269a69fb27b2f9473b362f5bb8e78d8`
- bartowski/Llama-3.2-3B-Instruct-GGUF `5ab33fa94d1d04e903623ae72c95d1696f09f9e8`
- bartowski/gemma-2-2b-it-GGUF `855f67caed130e1befc571b52bd181be2e858883`
- bartowski/Phi-3.5-mini-instruct-GGUF `6d70da17e749a471ccb62ade694486011a75cda3`

## Software

Parlato is a fork of Parla by Florian (https://github.com/LitteRabbit-37),
itself a Windows re-implementation of VoiceInk
(https://github.com/Beingpax/VoiceInk). Both are GPL-3.0. Speech and language
models run through whisper.cpp and llama.cpp (MIT), parakeet-rs and ort (MIT
or Apache-2.0) and ONNX Runtime (MIT). whisper.cpp runs inside Parlato;
llama.cpp runs inside the `parlato-llm` helper program that ships with
Parlato; ONNX Runtime is linked into Parlato.

### voice-engine

Speech and local AI, the model catalog, the shared model folder and the
downloader come from voice-engine, Copyright (c) Miguel Monzones, licensed
under the MIT License: https://github.com/mmnzns/voice-engine (the
`parlato-llm` helper is built from it).
