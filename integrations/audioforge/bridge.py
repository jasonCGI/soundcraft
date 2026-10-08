"""Local AudioForge speech bridge. Read settings from stdin; write a WAV take."""
import argparse
import io
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
import wave
from pathlib import Path

MAX_AUDIO = 25 * 1024 * 1024

def generate(settings, endpoint, output):
    address = urllib.parse.urlsplit(endpoint)
    if address.scheme != "http" or address.hostname != "127.0.0.1" or address.username or address.password:
        raise ValueError("Use a local AudioForge endpoint: http://127.0.0.1:PORT")
    if address.path not in ("", "/") or address.query or address.fragment:
        raise ValueError("Use a base URL without a path or query")
    text = settings.get("input", "")
    if not isinstance(text, str) or not text.strip() or len(text) > 5000:
        raise ValueError("Enter between 1 and 5000 characters")
    voice = settings.get("voice", "af_heart")
    if not isinstance(voice, str) or not voice or len(voice) > 80:
        raise ValueError("Enter a voice ID")
    speed = settings.get("speed", 1.0)
    if not isinstance(speed, (int, float)) or not 0.5 <= speed <= 2.0:
        raise ValueError("Speed must be between 0.5 and 2.0")
    payload = dict(model="kokoro", input=text, voice=voice, speed=speed, response_format="wav")
    request = urllib.request.Request(endpoint.rstrip("/") + "/v1/audio/speech", data=json.dumps(payload).encode(), headers={"Content-Type": "application/json"})
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    try:
        with opener.open(request, timeout=120) as response:
            audio = response.read(MAX_AUDIO + 1)
    except urllib.error.HTTPError as error:
        raise ValueError(f"AudioForge returned HTTP {error.code}. Check the service and voice ID.") from error
    except (urllib.error.URLError, TimeoutError) as error:
        raise ValueError("AudioForge is unavailable or timed out. Start the local TTS service.") from error
    if len(audio) > MAX_AUDIO:
        raise ValueError("The generated take exceeds 25 MB")
    with wave.open(io.BytesIO(audio), "rb") as wav:
        if wav.getnframes() == 0 or wav.getnchannels() > 2:
            raise ValueError("AudioForge returned empty or unsupported audio")
        if len(wav.readframes(wav.getnframes())) != wav.getnframes() * wav.getnchannels() * wav.getsampwidth():
            raise ValueError("AudioForge returned a truncated WAV")
    target = Path(output)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open("xb") as destination:
        destination.write(audio)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--endpoint", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    try:
        generate(json.loads(sys.stdin.read(32000)), args.endpoint, args.out)
    except (ValueError, OSError, wave.Error, EOFError, AttributeError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0

if __name__ == "__main__":
    sys.exit(main())
