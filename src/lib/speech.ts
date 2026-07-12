export const MAX_SPEAKABLE_CHARS = 200;

export function canSpeak(text: string, lang: string | null): boolean {
  const trimmed = text.trim();
  return (
    trimmed.length > 0 &&
    trimmed.length <= MAX_SPEAKABLE_CHARS &&
    lang !== null &&
    lang !== "auto"
  );
}

export function createPlayer() {
  let current: HTMLAudioElement | null = null;
  return {
    play(mp3Base64: string, onEnded: () => void) {
      current?.pause();
      current = new Audio(`data:audio/mpeg;base64,${mp3Base64}`);
      current.onended = onEnded;
      current.onerror = onEnded;
      void current.play();
    },
    stop() {
      current?.pause();
      current = null;
    },
  };
}
