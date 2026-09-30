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
(MIT).

## AI cleanup models (on this PC)

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
  non-commercial use only (research or evaluation).
- Built with Llama. Llama 3.2 is licensed under the Llama 3.2 Community
  License, Copyright (c) Meta Platforms, Inc. All Rights Reserved.

GGUF conversions of the Qwen, Llama, Gemma and Phi models by bartowski
(https://huggingface.co/bartowski); Granite GGUF published by IBM.

## Software

Parlato is a fork of Parla by Florian (https://github.com/LitteRabbit-37),
itself a Windows re-implementation of VoiceInk
(https://github.com/Beingpax/VoiceInk). Both are GPL-3.0. Speech and language
models run through whisper.cpp and llama.cpp (MIT), parakeet-rs and ort (MIT
or Apache-2.0) and ONNX Runtime (MIT).
