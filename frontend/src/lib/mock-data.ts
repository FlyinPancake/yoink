// Mock data mirroring the yoink-shared Rust models.
// This will be replaced with real API calls later.

export type Artist = {
  id: string;
  name: string;
  imageUrl?: string;
  monitored: boolean;
  albumCount: number;
  trackCount: number;
  addedAt: string;
};

export type Album = {
  id: string;
  artistId: string;
  artistName: string;
  title: string;
  albumType?: string;
  releaseDate?: string;
  coverUrl?: string;
  explicit: boolean;
  monitored: boolean;
  acquired: boolean;
  wanted: boolean;
  trackCount: number;
  addedAt: string;
};

export type Track = {
  id: string;
  title: string;
  albumId: string;
  albumTitle: string;
  artistId: string;
  artistName: string;
  discNumber: number;
  trackNumber: number;
  durationDisplay: string;
  explicit: boolean;
  monitored: boolean;
  acquired: boolean;
  filePath?: string;
};

export type DownloadStatus =
  | "queued"
  | "resolving"
  | "downloading"
  | "completed"
  | "failed";

export type DownloadJob = {
  id: string;
  albumId: string;
  albumTitle: string;
  artistName: string;
  source: string;
  status: DownloadStatus;
  quality: string;
  totalTracks: number;
  completedTracks: number;
  error?: string;
  createdAt: string;
  updatedAt: string;
};

// ── Mock artists ────────────────────────────────────────────

export const artists: Artist[] = [
  {
    id: "a1",
    name: "Radiohead",
    imageUrl:
      "https://i.scdn.co/image/ab6761610000e5eba03696716c9ee605006047fd",
    monitored: true,
    albumCount: 9,
    trackCount: 101,
    addedAt: "2025-01-15T10:30:00Z",
  },
  {
    id: "a2",
    name: "Tame Impala",
    imageUrl:
      "https://i.scdn.co/image/ab6761610000e5eb22e2e243fc7972b38e87259c",
    monitored: true,
    albumCount: 4,
    trackCount: 47,
    addedAt: "2025-02-20T14:00:00Z",
  },
  {
    id: "a3",
    name: "Khruangbin",
    imageUrl:
      "https://i.scdn.co/image/ab6761610000e5eb5a57c5e2095bf2f0d6aadf38",
    monitored: true,
    albumCount: 3,
    trackCount: 33,
    addedAt: "2025-03-01T09:15:00Z",
  },
  {
    id: "a4",
    name: "Floating Points",
    monitored: true,
    albumCount: 3,
    trackCount: 28,
    addedAt: "2025-03-10T16:45:00Z",
  },
  {
    id: "a5",
    name: "King Gizzard & The Lizard Wizard",
    imageUrl:
      "https://i.scdn.co/image/ab6761610000e5eb079f262348ac50ff55574acf",
    monitored: false,
    albumCount: 2,
    trackCount: 22,
    addedAt: "2025-04-05T11:20:00Z",
  },
  {
    id: "a6",
    name: "Bonobo",
    imageUrl:
      "https://i.scdn.co/image/ab6761610000e5eb93a2b710c1f0e4584aab5c00",
    monitored: true,
    albumCount: 5,
    trackCount: 58,
    addedAt: "2025-01-22T08:00:00Z",
  },
];

// ── Mock albums ─────────────────────────────────────────────

export const albums: Album[] = [
  {
    id: "al1",
    artistId: "a1",
    artistName: "Radiohead",
    title: "OK Computer",
    albumType: "Album",
    releaseDate: "1997-05-21",
    coverUrl:
      "https://i.scdn.co/image/ab67616d0000b273c8b444df094279e70d0ed856",
    explicit: false,
    monitored: true,
    acquired: true,
    wanted: false,
    trackCount: 12,
    addedAt: "2025-01-15T10:30:00Z",
  },
  {
    id: "al2",
    artistId: "a1",
    artistName: "Radiohead",
    title: "In Rainbows",
    albumType: "Album",
    releaseDate: "2007-10-10",
    coverUrl:
      "https://i.scdn.co/image/ab67616d0000b2737ebe94725895bdc8e4fec928",
    explicit: false,
    monitored: true,
    acquired: false,
    wanted: true,
    trackCount: 10,
    addedAt: "2025-01-15T10:30:00Z",
  },
  {
    id: "al3",
    artistId: "a2",
    artistName: "Tame Impala",
    title: "Currents",
    albumType: "Album",
    releaseDate: "2015-07-17",
    coverUrl:
      "https://i.scdn.co/image/ab67616d0000b2739e1cfc756886ac782e363d79",
    explicit: false,
    monitored: true,
    acquired: true,
    wanted: false,
    trackCount: 13,
    addedAt: "2025-02-20T14:00:00Z",
  },
  {
    id: "al4",
    artistId: "a2",
    artistName: "Tame Impala",
    title: "The Slow Rush",
    albumType: "Album",
    releaseDate: "2020-02-14",
    coverUrl:
      "https://i.scdn.co/image/ab67616d0000b273b267d2170418e1c5239fa42b",
    explicit: false,
    monitored: true,
    acquired: false,
    wanted: true,
    trackCount: 12,
    addedAt: "2025-02-20T14:00:00Z",
  },
  {
    id: "al5",
    artistId: "a3",
    artistName: "Khruangbin",
    title: "Con Todo El Mundo",
    albumType: "Album",
    releaseDate: "2018-01-26",
    coverUrl:
      "https://i.scdn.co/image/ab67616d0000b273aa8dd1e3716e0884a0366a8c",
    explicit: false,
    monitored: true,
    acquired: true,
    wanted: false,
    trackCount: 11,
    addedAt: "2025-03-01T09:15:00Z",
  },
  {
    id: "al6",
    artistId: "a6",
    artistName: "Bonobo",
    title: "Migration",
    albumType: "Album",
    releaseDate: "2017-01-13",
    coverUrl:
      "https://i.scdn.co/image/ab67616d0000b273c9b3be87a0b6b0f0e2e3b2f8",
    explicit: false,
    monitored: true,
    acquired: false,
    wanted: true,
    trackCount: 12,
    addedAt: "2025-01-22T08:00:00Z",
  },
  {
    id: "al7",
    artistId: "a1",
    artistName: "Radiohead",
    title: "Kid A",
    albumType: "Album",
    releaseDate: "2000-10-02",
    explicit: false,
    monitored: true,
    acquired: true,
    wanted: false,
    trackCount: 11,
    addedAt: "2025-01-16T10:00:00Z",
  },
  {
    id: "al8",
    artistId: "a4",
    artistName: "Floating Points",
    title: "Crush",
    albumType: "Album",
    releaseDate: "2019-10-18",
    explicit: false,
    monitored: true,
    acquired: false,
    wanted: true,
    trackCount: 12,
    addedAt: "2025-03-10T16:45:00Z",
  },
];

// ── Mock tracks (subset for library view) ───────────────────

export const tracks: Track[] = [
  {
    id: "t1",
    title: "Airbag",
    albumId: "al1",
    albumTitle: "OK Computer",
    artistId: "a1",
    artistName: "Radiohead",
    discNumber: 1,
    trackNumber: 1,
    durationDisplay: "4:44",
    explicit: false,
    monitored: true,
    acquired: true,
    filePath: "/music/Radiohead/OK Computer/01 - Airbag.flac",
  },
  {
    id: "t2",
    title: "Paranoid Android",
    albumId: "al1",
    albumTitle: "OK Computer",
    artistId: "a1",
    artistName: "Radiohead",
    discNumber: 1,
    trackNumber: 2,
    durationDisplay: "6:23",
    explicit: false,
    monitored: true,
    acquired: true,
    filePath: "/music/Radiohead/OK Computer/02 - Paranoid Android.flac",
  },
  {
    id: "t3",
    title: "Subterranean Homesick Alien",
    albumId: "al1",
    albumTitle: "OK Computer",
    artistId: "a1",
    artistName: "Radiohead",
    discNumber: 1,
    trackNumber: 3,
    durationDisplay: "4:27",
    explicit: false,
    monitored: true,
    acquired: true,
    filePath:
      "/music/Radiohead/OK Computer/03 - Subterranean Homesick Alien.flac",
  },
  {
    id: "t4",
    title: "Let Down",
    albumId: "al1",
    albumTitle: "OK Computer",
    artistId: "a1",
    artistName: "Radiohead",
    discNumber: 1,
    trackNumber: 5,
    durationDisplay: "4:59",
    explicit: false,
    monitored: true,
    acquired: true,
  },
  {
    id: "t5",
    title: "Karma Police",
    albumId: "al1",
    albumTitle: "OK Computer",
    artistId: "a1",
    artistName: "Radiohead",
    discNumber: 1,
    trackNumber: 6,
    durationDisplay: "4:21",
    explicit: false,
    monitored: true,
    acquired: true,
  },
  {
    id: "t6",
    title: "15 Step",
    albumId: "al2",
    albumTitle: "In Rainbows",
    artistId: "a1",
    artistName: "Radiohead",
    discNumber: 1,
    trackNumber: 1,
    durationDisplay: "3:57",
    explicit: false,
    monitored: true,
    acquired: false,
  },
  {
    id: "t7",
    title: "Bodysnatchers",
    albumId: "al2",
    albumTitle: "In Rainbows",
    artistId: "a1",
    artistName: "Radiohead",
    discNumber: 1,
    trackNumber: 2,
    durationDisplay: "4:02",
    explicit: false,
    monitored: true,
    acquired: false,
  },
  {
    id: "t8",
    title: "Reckoner",
    albumId: "al2",
    albumTitle: "In Rainbows",
    artistId: "a1",
    artistName: "Radiohead",
    discNumber: 1,
    trackNumber: 7,
    durationDisplay: "4:50",
    explicit: false,
    monitored: true,
    acquired: false,
  },
  {
    id: "t9",
    title: "Let It Happen",
    albumId: "al3",
    albumTitle: "Currents",
    artistId: "a2",
    artistName: "Tame Impala",
    discNumber: 1,
    trackNumber: 1,
    durationDisplay: "7:46",
    explicit: false,
    monitored: true,
    acquired: true,
    filePath: "/music/Tame Impala/Currents/01 - Let It Happen.flac",
  },
  {
    id: "t10",
    title: "The Less I Know the Better",
    albumId: "al3",
    albumTitle: "Currents",
    artistId: "a2",
    artistName: "Tame Impala",
    discNumber: 1,
    trackNumber: 7,
    durationDisplay: "3:36",
    explicit: false,
    monitored: true,
    acquired: true,
    filePath:
      "/music/Tame Impala/Currents/07 - The Less I Know the Better.flac",
  },
  {
    id: "t11",
    title: "One More Year",
    albumId: "al4",
    albumTitle: "The Slow Rush",
    artistId: "a2",
    artistName: "Tame Impala",
    discNumber: 1,
    trackNumber: 1,
    durationDisplay: "5:23",
    explicit: false,
    monitored: true,
    acquired: false,
  },
  {
    id: "t12",
    title: "Breathe Deeper",
    albumId: "al4",
    albumTitle: "The Slow Rush",
    artistId: "a2",
    artistName: "Tame Impala",
    discNumber: 1,
    trackNumber: 4,
    durationDisplay: "6:12",
    explicit: false,
    monitored: true,
    acquired: false,
  },
  {
    id: "t13",
    title: "Maria También",
    albumId: "al5",
    albumTitle: "Con Todo El Mundo",
    artistId: "a3",
    artistName: "Khruangbin",
    discNumber: 1,
    trackNumber: 1,
    durationDisplay: "4:07",
    explicit: false,
    monitored: true,
    acquired: true,
  },
  {
    id: "t14",
    title: "Evan Finds the Third Room",
    albumId: "al5",
    albumTitle: "Con Todo El Mundo",
    artistId: "a3",
    artistName: "Khruangbin",
    discNumber: 1,
    trackNumber: 2,
    durationDisplay: "3:32",
    explicit: false,
    monitored: true,
    acquired: true,
  },
];

// ── Mock downloads ──────────────────────────────────────────

export const downloads: DownloadJob[] = [
  {
    id: "d1",
    albumId: "al2",
    albumTitle: "In Rainbows",
    artistName: "Radiohead",
    source: "tidal",
    status: "downloading",
    quality: "Lossless",
    totalTracks: 10,
    completedTracks: 6,
    createdAt: "2025-04-10T14:00:00Z",
    updatedAt: "2025-04-10T14:05:30Z",
  },
  {
    id: "d2",
    albumId: "al4",
    albumTitle: "The Slow Rush",
    artistName: "Tame Impala",
    source: "tidal",
    status: "queued",
    quality: "Lossless",
    totalTracks: 12,
    completedTracks: 0,
    createdAt: "2025-04-10T14:01:00Z",
    updatedAt: "2025-04-10T14:01:00Z",
  },
  {
    id: "d3",
    albumId: "al6",
    albumTitle: "Migration",
    artistName: "Bonobo",
    source: "deezer",
    status: "resolving",
    quality: "Lossless",
    totalTracks: 12,
    completedTracks: 0,
    createdAt: "2025-04-10T13:55:00Z",
    updatedAt: "2025-04-10T13:55:10Z",
  },
  {
    id: "d4",
    albumId: "al8",
    albumTitle: "Crush",
    artistName: "Floating Points",
    source: "soulseek",
    status: "failed",
    quality: "Lossless",
    totalTracks: 12,
    completedTracks: 3,
    error: "Connection timed out after 3 retries",
    createdAt: "2025-04-09T20:00:00Z",
    updatedAt: "2025-04-09T20:15:00Z",
  },
  {
    id: "d5",
    albumId: "al7",
    albumTitle: "Kid A",
    artistName: "Radiohead",
    source: "tidal",
    status: "completed",
    quality: "HiRes",
    totalTracks: 11,
    completedTracks: 11,
    createdAt: "2025-04-08T10:00:00Z",
    updatedAt: "2025-04-08T10:12:00Z",
  },
];

// ── Dashboard helpers ───────────────────────────────────────

export const stats = {
  totalArtists: artists.length,
  totalAlbums: albums.length,
  totalTracks: tracks.length,
  wantedAlbums: albums.filter((a) => a.wanted).length,
  acquiredAlbums: albums.filter((a) => a.acquired).length,
  activeDownloads: downloads.filter((d) =>
    ["queued", "resolving", "downloading"].includes(d.status),
  ).length,
};
