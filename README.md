# yt_video_shorts_generator
Generate Youtube shorts based from Original video based on content semantics

Create the folder models in the root of the directory:
mkdir models

Download the model using the following command:
curl -L -o models/ggml-large-v3-turbo.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin

Create GCP project:
https://console.cloud.google.com/

Enable drive api:
https://console.cloud.google.com/apis/library/drive.googleapis.com

Generate api key in google studios
https://aistudio.google.com/app/api-keys?project=gen-lang-client-0317339696
Create .env file in the root of the project and place it like this GEMINI_API_KEY=xxxxxxxxxxxxxxxxxxxxxxxxxxxxx


Or go with the Docker:
docker run --rm -p 3000:3000 \
  -e GEMINI_API_KEY="$GEMINI_API_KEY" \
  -v "$(pwd)/service_account.json:/app/service_account.json:ro" \
  -v "$(pwd)/downloads:/app/downloads" \
  short-gen
