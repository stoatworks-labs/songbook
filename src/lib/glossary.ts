import type { Platform } from '../types';

export interface GlossaryEntry {
  term: string;
  text: string;
  platforms?: Platform[];
}

/** Plain-English notes for the documentation's "what the settings mean" page. */
export const GLOSSARY: GlossaryEntry[] = [
  { term: 'Socket', text: 'A physical connector on the desk, a stage box or an audio network card: a mic/line XLR, an AES pair, a Dante or S-Link channel, a USB stream. Sockets exist whether or not anything is patched to them.' },
  { term: 'Patch', text: 'Which socket feeds which channel, and which bus or direct out leaves on which output socket. The patch is separate from the channel itself: renaming a channel does not move the cable, and repatching does not rename it.' },
  { term: 'Preamp (gain, pad, 48V)', text: 'The analogue stage in front of the converter: gain sets the level arriving at the desk, the pad drops a hot source before it clips, and 48V phantom powers condenser microphones and active DI boxes. On a shared stage box these belong to the socket, not the channel, so two desks sharing it share the gain.' },
  { term: 'Trim / digital gain', text: 'A gain change after the converter, per channel. On a shared stage box it is the only gain a guest desk can move without affecting the other.' },
  { term: 'Fader / on', text: 'The channel level sent to the main mix and post-fade sends, and whether the channel is muted. A fader at -inf and a muted channel sound the same but read differently on the desk.' },
  { term: 'Pan / balance', text: 'The position between left and right. On a mono channel it pans the signal; on a stereo channel it balances the two legs.' },
  { term: 'Send (pre / post)', text: 'The level from a channel to an aux, group or FX bus. A pre-fade send ignores the channel fader (monitors on stage keep their level when front of house rides the fader); a post-fade send follows it (reverb stays in proportion).' },
  { term: 'Aux / group / matrix', text: 'An aux is a mix built from per-channel sends, typically a monitor wedge or IEM. A group collects whole channels routed into it. A matrix is a mix of mixes — main, groups and auxes — used for delay towers, broadcast and record feeds.' },
  { term: 'DCA / VCA', text: 'A fader that controls the level of its member channels without carrying their audio. Moving a DCA moves every member; the members keep their own relative levels and their own sends.' },
  { term: 'Mute group', text: 'A button that mutes its members together. Unlike a DCA it has no level: it only mutes.' },
  { term: 'HPF / LPF', text: 'A high-pass filter removes rumble, handling noise and stage spill below its frequency; a low-pass removes hiss and cymbal spill above it. On most desks the HPF is the first processing in the channel.' },
  { term: 'EQ', text: 'Per-band tone shaping: each band has a frequency, a gain and a Q (how wide it is). Shelves lift or cut everything above or below their frequency; bells work around it; a notch removes a narrow problem such as feedback.' },
  { term: 'Gate', text: 'Closes the channel while the signal is below the threshold, so a drum microphone does not carry the rest of the kit. Range says how far it closes; hold and release say how long it takes to close again.' },
  { term: 'Compressor', text: 'Reduces level above the threshold by the ratio, so the loud parts come down and the mix stays even. Attack and release say how fast it reacts; the knee says how gradually it starts.' },
  { term: 'Scene / snapshot', text: 'A stored state of the desk, recalled between songs, acts or presentations. What a scene recalls depends on its recall filters — a scene may carry levels but not the preamp gains, so a shared stage box is not disturbed.' },
  { term: 'Recall filter / safe', text: 'What a scene is allowed to change when it is recalled. A channel made "safe" keeps its current state through every recall — usually a presenter microphone or a playback channel.' },
  { term: 'Cue list', text: 'An ordered list of scene recalls for the show, with the running order that the operator follows. Songbook documents it; the desk plays it.' },
  { term: 'Vendor file', text: "The desk's own show file — an SQ show folder, a CL/QL .CLF, a DM3 or TF scene archive. Songbook keeps it next to its own model so a show can go back to the same desk exactly as it came off." },
];
