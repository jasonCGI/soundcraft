"""Separate a local PCM16 WAV into aligned estimated stems using TorchAudio.
No third-party audio is bundled. Model weights download from PyTorch on first use.
"""
import argparse
import array
import hashlib
import json
import sys
import wave
from pathlib import Path


def inspect_input(path):
    path = Path(path)
    if path.stat().st_size > 25 * 1024 * 1024:
        raise ValueError('Input exceeds 25 MB')
    with wave.open(str(path), 'rb') as audio:
        rate, channels, frames = audio.getframerate(), audio.getnchannels(), audio.getnframes()
        if audio.getsampwidth() != 2 or channels not in (1, 2) or not 8000 <= rate <= 192000:
            raise ValueError('Use mono or stereo PCM16 WAV')
        if frames < 1 or frames / rate > 60:
            raise ValueError('Use up to 60 seconds of audio')
        values = array.array('h', audio.readframes(frames))
        if sys.byteorder != 'little':
            values.byteswap()
        if len(values) != frames * channels:
            raise ValueError('Truncated WAV')
    return rate, channels, frames, values


def separate(input_path, output_path, cache=None):
    rate, channels, frames, values = inspect_input(input_path)
    out = Path(output_path)
    if out.exists():
        raise ValueError('Choose a new output folder')
    import torch
    import torchaudio
    torch.set_num_threads(4)
    if cache:
        torch.hub.set_dir(str(Path(cache)))
    bundle = torchaudio.pipelines.HDEMUCS_HIGH_MUSDB_PLUS
    model = bundle.get_model().eval().to('cpu')
    mix = torch.tensor(values, dtype=torch.float32).reshape(frames, channels).T / 32768
    if channels == 1:
        mix = mix.repeat(2, 1)
    mix = torchaudio.functional.resample(mix, rate, bundle.sample_rate)
    length = mix.shape[-1]
    mean, scale = mix.mean(), mix.std().clamp_min(1e-5)
    mix = (mix - mean) / scale
    segment, overlap = bundle.sample_rate * 8, bundle.sample_rate
    step = segment - overlap
    result = torch.zeros((len(model.sources), 2, length))
    weights = torch.zeros(length)
    with torch.inference_mode():
        for start in range(0, length, step):
            end = min(length, start + segment)
            predicted = model(mix[:, start:end].unsqueeze(0))[0]
            window = torch.ones(end - start)
            fade = min(overlap, end - start)
            if start > 0:
                window[:fade] = torch.linspace(1e-4, 1, fade)
            if end < length:
                window[-fade:] = torch.linspace(1, 1e-4, fade)
            result[:, :, start:end] += predicted * window
            weights[start:end] += window
            print(f'Separating {end / bundle.sample_rate:.1f}s / {length / bundle.sample_rate:.1f}s', flush=True)
    result = result / weights.clamp_min(1e-5)
    # Restore one share of DC offset per stem so summing does not multiply it.
    result = result * scale + mean / len(model.sources)
    if not torch.isfinite(result).all():
        raise ValueError('Model produced non-finite audio')
    # Shared gain preserves the relative levels and prevents individual clipping.
    gain = min(1.0, 0.95 / max(float(result.abs().max()), 1e-5))
    result *= gain
    out.mkdir(parents=False, exist_ok=False)
    manifest = dict(schema=1, method='torchaudio-hdemucs-high-musdb-plus', estimated=True,
                    input_sha256=hashlib.sha256(Path(input_path).read_bytes()).hexdigest(),
                    sample_rate=bundle.sample_rate, frames=length, start_sample=0, gain=gain,
                    note='Estimated stems may contain bleed. Other includes guitars and synths; vocals are not split by singer.', stems=[])
    for index, name in enumerate(model.sources):
        destination = out / (name + '.wav')
        samples = array.array('h', (result[index].T.flatten().clamp(-1, 1) * 32767).round().to(torch.int16).tolist())
        if sys.byteorder != 'little':
            samples.byteswap()
        with destination.open('xb') as file:
            with wave.open(file, 'wb') as audio:
                audio.setnchannels(2); audio.setsampwidth(2); audio.setframerate(bundle.sample_rate)
                audio.writeframes(samples.tobytes())
        manifest['stems'].append(dict(name=name, path=str(destination.resolve()), estimated=True))
    (out / 'stems.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
    return manifest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--input', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--cache')
    args = parser.parse_args()
    try:
        result = separate(args.input, args.out, args.cache)
        print(json.dumps(result), flush=True)
    except Exception as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
