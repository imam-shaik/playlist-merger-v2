export interface TranslatedError {
  userMessage: string;
  suggestion?: string;
  details: string;
}

const KNOWN_PATTERNS: Array<{
  test: RegExp;
  translate: (match: RegExpExecArray) => TranslatedError;
}> = [
  {
    test: /moov atom not found/i,
    translate: () => ({
      userMessage: 'This video appears to be damaged or incomplete and cannot be processed.',
      suggestion: 'Try re-encoding the file or removing it from the playlist.',
      details: 'moov atom not found',
    }),
  },
  {
    test: /PTS corruption/i,
    translate: (m) => ({
      userMessage: 'Critical timing corruption detected in one of your video files.',
      suggestion: 'Run Health Check on your files, or try Smart Audio Repair mode.',
      details: m[0],
    }),
  },
  {
    test: /non-?monotonous\s+(DTS|timestamp)/i,
    translate: (m) => ({
      userMessage: 'A video file has inconsistent internal timing information.',
      suggestion: 'Try using Smart Audio Repair mode or re-encode the problematic file.',
      details: m[0],
    }),
  },
  {
    test: /non-?monotonous\s+timestamps/i,
    translate: (m) => ({
      userMessage: 'A video file has inconsistent internal timing information.',
      suggestion: 'Try using Smart Audio Repair mode or re-encode the problematic file.',
      details: m[0],
    }),
  },
  {
    test: /Invalid\s+(DTS|PTS|timestamp)/i,
    translate: (m) => ({
      userMessage: 'A video file has invalid timing data that prevents processing.',
      suggestion: 'Check the file with Health Check or remove it from the playlist.',
      details: m[0],
    }),
  },
  {
    test: /missing picture in access unit/i,
    translate: () => ({
      userMessage: 'A codec transition in your playlist causes playback corruption.',
      suggestion: 'Use Custom mode and force a consistent video codec across all files.',
      details: 'missing picture in access unit',
    }),
  },
  {
    test: /timed out after/i,
    translate: () => ({
      userMessage: 'Processing took too long and timed out.',
      suggestion: 'Try with fewer files, shorter durations, or use a faster codec preset.',
      details: 'FFmpeg process timed out',
    }),
  },
  {
    test: /audio repair failed.*file #(\d+)/i,
    translate: (m) => ({
      userMessage: `Audio repair failed for file #${m[1]}. The audio stream may be too damaged to recover.`,
      suggestion: 'Remove the problematic file from the playlist and try again.',
      details: m[0],
    }),
  },
  {
    test: /zero.duration/i,
    translate: () => ({
      userMessage: 'A file has zero duration and cannot be processed.',
      suggestion: 'Remove the file from the playlist or re-encode it first.',
      details: 'Zero-duration file detected',
    }),
  },
  {
    test: /Critical profile mismatches/i,
    translate: () => ({
      userMessage: 'Some files have incompatible video/audio formats that could not be resolved.',
      suggestion: 'Run Health Check to see which files differ, or use Custom mode to force a consistent format.',
      details: 'Critical profile mismatches remain before concat',
    }),
  },
  {
    test: /Cannot open subtitle/i,
    translate: () => ({
      userMessage: 'A subtitle file could not be read or is in an unsupported format.',
      suggestion: 'Ensure the subtitle file is accessible and in a valid format (SRT, ASS, VTT).',
      details: 'Subtitle file error',
    }),
  },
  {
    test: /Insufficient disk space/i,
    translate: (m) => ({
      userMessage: m[0],
      suggestion: 'Free up disk space or choose a different output location.',
      details: m[0],
    }),
  },
  {
    test: /file\(s\) failed basic health checks/i,
    translate: (m) => ({
      userMessage: m[0],
      suggestion: 'Run Health Check (Compatibility tab) to identify and remove problematic files.',
      details: m[0],
    }),
  },
  {
    test: /Audio.channel mismatch/i,
    translate: () => ({
      userMessage: 'Some files have more than 2 audio channels, which can cause playback issues.',
      suggestion: 'Use Audio Repair mode to normalise audio channels automatically.',
      details: 'Audio channel mismatch detected',
    }),
  },
  {
    test: /ffmpeg not found/i,
    translate: () => ({
      userMessage: 'FFmpeg could not be found on your system.',
      suggestion: 'Install FFmpeg from https://ffmpeg.org/download.html or set a custom path in Settings.',
      details: 'FFmpeg executable not found',
    }),
  },
];

export function translateError(error: string): TranslatedError {
  for (const pattern of KNOWN_PATTERNS) {
    const match = pattern.test.exec(error);
    if (match) {
      return pattern.translate(match);
    }
  }
  const truncated = error.length > 200 ? error.substring(0, 200) + '...' : error;
  return {
    userMessage: truncated,
    details: error,
  };
}
