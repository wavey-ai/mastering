# Mastering rules

Revision: `engineered-rules.27`.

These thresholds are explicit engineering hypotheses.
They are not established genre standards, learned parameters, or advice attributed to an engineer.
Tests establish implementation behavior; listening tests must establish musical usefulness.

## Intent mapping

Removed (D137). The set path has no genre or intensity control.

`intensity` selects a target by linear interpolation between the genre endpoints.

| Genre | Intensity 0 | Intensity 1 |
|---|---:|---:|
| Neutral | −16 LUFS | −6 LUFS |
| Pop | −15 LUFS | −6.25 LUFS |
| Rock | −15 LUFS | −6.25 LUFS |
| Electronic | −15 LUFS | −6 LUFS |
| Hip-hop | −15 LUFS | −6.25 LUFS |
| Acoustic | −20 LUFS | −13 LUFS |

The browser starts at neutral, intensity 0.8 and punch 0.5: a −8 LUFS aim.
These ranges provide initial control behavior.
Genre currently affects this range only.
It does not establish instrumentation, tempo, bass requirements, or a reference spectrum.

## Rule ledger

| Rule | Basis | Action |
|---|---|---|
| `intent.density.v3` | User genre and intensity | Removed (D137). Select the requested loudness range position. |
| `preserve.punch.v2` | User punch; measured before/after contrasts | Limit peak reduction and reject excessive contrast loss. |
| `imaging.eligibility.v1` | Source band energy, correlation, coherence, coverage, and variation | Removed (D137). Propose bounded side-filter trials; compare complete renders with an unchanged-width master. |
| `source.clipping.v1` | Full-scale occupancy or candidate flat-top runs | Removed (D137). Report the source condition; measure any further peak reduction against attack, crest, contrast, and limiter limits. |
| `intent.character.v1` | Explicit character choice | Removed (D137). Apply the bounded tone filters below. |
| `vocal.caution.v1` | User declares vocals | Removed (D137). Reduce bright-character boosts; imaging also uses a tighter center-loss limit. Do not infer vocal identity. |
| `dynamics.release.v2` | Starting recovery without reliable instrument timing | Removed (D137). Use 50 ms; sparse full-mix onsets do not establish a slow release. |
| `dynamics.stem-release.v1` | Validated drum decay and bass-body estimates | Removed (D137). Half the median 6 dB drum decay, at least two cycles of the bass body, bounded 35–80 ms. The set versions use `set.limiter-release.v1` (D127). |
| `compression.eligibility.v3` | Onset evidence, crest, requested intensity and measured limiter workload | Removed (D137). Render fresh full-band and bass-reduced detector trials; accept only measured matched-level benefit within source and spatial limits. Flat-top indicators remain reported. |
| `loudness.search.v1` | Final peak-trimmed output loudness and available reduction budget | Search bounded drive values; report every probe and the stop reason. |
| `finishing.rules.1` | Relative-band statistics, DC, active-window levels, character intent, and complete-chain results | Removed (D137). Compare bounded finishing stages with the retained base chain. |
| `finishing.selection.4` | Eligible stage families and complete-chain comparison results | Removed (D137). Test the combined bundle, then isolated alternatives after rejection; retain the first passing candidate or exact base audio; re-search toward the requested loudness after a passing stage frees headroom. |
| `stem-evidence.2` | Aligned, unnormalized supplied drums/bass/other/vocals estimates | Measure drum decays, all-role one-third-octave distributions and vocal activity; reject unreliable source matches. |
| `finishing.stem-bands.2` | Bass body and vocal-associated treble plus corroborating original-mix excess | Removed (D137). Test stronger combined then isolated dynamic bands before generic finishing candidates. |
| `album.source-relative.v1` | Source and mastered integrated levels across ordered tracks | Removed (D137). Apply only final attenuation to preserve original relative levels. |
| `set.unchanged.v2` | Original stereo source | Keep unchanged tonal/spatial processing eligible; render essential peak control through the A–F treatment chain. |
| `set.all-role-foundation.v1` | Source-bound aligned estimates of drums, bass, other and vocals, plus local reconstruction against the original mix | Select at most one supported adjacent-band mid/side lift per role, combine nonoverlapping regions, and vary each filter over reliable two-second passages on the original stereo mix. Measured, not applied: its bells change the mix by 0.06 to 0.31 dB, so no set version uses it (D128). |
| `set.source-density.v4` | Source 50 ms crest distribution, 12 s window contrast and onsets | Base target −(crest median + half the contrast above 6 dB) moved by the difference between the delivery ceiling and −0.5 dBTP (D152, D155), bounded −20 to −8 LUFS; denser spacing from the crest span. `set.audible-loudness.v3` caps these targets. |
| `set.audible-loudness.v3` | Total masked NMR of the set's peak control on the compressed mix at each probe drive | The audibility point is the output loudness at which the total NMR reaches 0 dB. It caps the `set.source-density.v4` targets: the base target at most 2.75 LU under it, where the album masters of D177 sit (D179), the Denser target at most at it, with the −6.5 LUFS cap and the base render's limiter headroom. The base render of the planner searches its drive and joins the probes (`audibility::render_at`). A source that arrives limited keeps `set.input-condition.v1` (D131). Provisional. |
| `set.focus-strength.v1` | Actual candidate-filter responses on aligned vocal/accompaniment estimates | Interpolate a bounded gain from the measured response; retain study strength without sufficient evidence. |
| `set.stem-recovery.v1` | Drum decay, event spacing and bass period | Derive bounded peak recovery, retaining 50 ms without reliable timing evidence. The set versions use `set.limiter-release.v1` (D127). |
| `set.focus-passages.v1` | Local reconstruction, vocal activity, overlap and filter response | Slowly vary focus contribution over supported passages; always process the original stereo mix. |
| `set.result-name.v1` | Achieved matched band powers, side/mid balance and limiter workload | Name the completed result in this recording; keep stable IDs separate. |
| `set.peak-share.v3` | Version density | Deliver at −0.5 dBTP (D155); bound the limiter 1.5 dB (base) or 2.5 dB (denser) above the ceiling and soft-clip the remainder onto it. Provisional. |
| `set.split-band.v1` | Zero-phase low band of the mix at a crossover from the stems | Rejected (D121). The wideband limiter and the soft clipper are the peak control. |
| `set.broad-tone.v6` | ISO 226 equal-loudness contours at the delivered level change; the lead role's share of the arrangement per region from aligned stems | Propose a bass and an air shelf that keep the perceived balance of the mix at the louder delivered level, a presence lift that restores the lead's share to parity with its body within the presence bound of `set.presence-balance.v2`, and a low-mid cut of the deficit, each up to ±2 dB. The shelves act on the stereo mix; the presence bell acts on the mid channel and is gated there (D128). The low-mid cut is dynamic (`set.low-mid.v1`). The lead is the vocal by precedence or the strongest midrange role. No corpus or reference sets a target. |
| `set.move-class.v1` | The kind of each tone move | The presence bells and the widening are character moves; the shelves, the low-mid cut, the focus bells, the side cut, the de-esser, the restoration and the low-end hold are corrections. `bold` keeps the corrections of `master` (D156) and makes its steps with `set.vivid-steps.v1` (D161). |
| `set.low-mid.v1` | Masking threshold of the other stems and power of the lead stem from 250 to 500 Hz in each masking frame | Cut a mid-channel band (Q 1.4 or more, lower −3 dB point at 200 Hz or higher, centre from the masked depths) in the frames where the lead is masked, by the masked depth up to the cut of the whole-track deficit; attack 0.1 s, release 0.5 s; `master` and `bold` (D158). Provisional. |
| `set.low-end.v3` | Power from 40 to 120 Hz and from 120 to 250 Hz in each 15 s section of the version and of the `essential` render, at matched integrated loudness | Where a band of `master` or `bold` is more than 0.5 dB below `essential`, less the intended move of the static low shelves of the version (the equal-loudness bass shelf, the bass shelves of `set.bass-version.v4` and the shelves of `set.capture-low-band.v1`): decrease the low-mid correction by its share, then lift the band with an enveloped bell (up to +2 dB) to −0.25 dB; the loudness match takes the loudness of the lift from every band, and no body cut pays for it (D185); the hold lifts no held band more than 0.5 dB above `essential`, and a band already within 0.25 dB of that limit or past it rises by at most 0.25 dB (`low_end::SPILL_DB`, D183); the envelope has the share of each section at its centre and glides between centres; each render of the hold also takes the next restoration pass, and the hold plans against the shift of that pass; a candidate that stays below fails (D159, D163, D183, D185). Provisional. |
| `set.master.v1` | Joint broad-tone proposal, strongest stem focus proposal, de-esser, low-mid correction and widening | Deliver `essential`, `master` and `bold` (D135); `master` has the bass shelves of `set.bass-version.v4` when the track has bass (D164, D182). `bold` is the chosen `master` candidate without the bass shelves, with the tone of `set.vivid-steps.v2` and the `bold` widening; its fallbacks are the midway widening, the `master` widening and the `master` tone (D156, D159, D161). When `master` chooses no candidate, `bold` builds from the planned first `master` candidate without the bass shelves (`set.bass-version.v4`). |
| `set.vivid-steps.v2` | Source spectrum; the tone and the side gain of `master`; the moves of the album pairs of D177 | The lead bells of `bold` keep the gains of `master`. The presence balance bell has the gain whose predicted presence against body is 1 dB above `master`, at least the gain of `master` and at most 6 dB; the predicted presence move from the source stays within the presence bound of `master` plus 0.48 dB, the P90 move of the pairs (+1.42 dB) less their P75 move (D180). Where the source band reaches 12 kHz, a 6 kHz high shelf puts the predicted treble against body 0.5 dB above `master`, at most 2 dB (D161, D162). Provisional. |
| `set.air-balance.v1` | Source spectrum; the static presence moves; the codec bandwidth of the source condition | After the presence moves of `master`, a high shelf at 8 kHz returns the predicted air (8 to 16 kHz) against presence (2 to 5 kHz) to that of the mix, at most 2 dB, from 0.5 dB, without a gate, where the source band reaches 16 kHz. `bold` solves it again for its presence (D162). Provisional. |
| `set.bass-version.v4` | Stem power shares of the drums and bass stems from 40 to 120 Hz; the long-term spectrum of the mix; the moves of the album pairs of D177; the tonal gates of the candidates | When the drums and bass stems hold more than half of the stem power from 40 to 120 Hz, each candidate of `master` carries the two stereo low shelves of `open::plan`: the median sub-against-bass (+1.4 dB) and bass-against-body (+1.2 dB) moves of the album pairs, solved on the spectrum of the mix, each reduced where the mix reaches the largest outcome of the pairs (D182). The low-end hold counts the shelves as intended moves, with bells of up to 2 dB, and its gate checks each section of the held bands at the floor of `set.low-end.v3`. The candidates with the shelves leave out the tonal gates of a band under 500 Hz against a band above it or the whole file (`low_end::lifts`, `set::bass_candidates`). When no candidate with the shelves passes, `master` renders the same candidates without them and is delivered as the plain `master` (`set::base_ladder`). The chosen index names the planned candidate in both ladders. `bold` builds from the chosen candidate without the shelves, or from the planned first `master` candidate when `master` chooses none. The finalizer keeps the speculative `bold` that passed its gates, and the native set renders that `bold`. Provisional. |
| `set.version-names.v1` | The six band powers, the bass of the held bands and the side-to-mid ratio of each delivered version and of the mix, at matched loudness | After the last version, each version takes a name from Warm, Smooth, Full, Forward, Bright, Airy, Wide, Focused, Vivid and Essential: a later version the name of its largest step from the version before it, the bass version Full when that step is 0.5 dB or more, and the version closest to the mix its largest margin over every other version, or Essential. A name needs 0.5 dB and occurs once in a set. A later version whose largest step of 0.5 dB or more is in a character that a version before it holds takes the comparative name, such as More forward, described against the version before it. A later version with no step of 0.5 dB or more in any character over the version before it is not delivered. The description states the character in plain words, and the figures move to `detail` (D165). Provisional. |
| `set.low-end-mono.v1` | Side share of source power below 100 Hz | Apply a second-order side high-pass at 100 Hz to every version when the share is at least 2%. Provisional. |
| `set.side-lift.v1` | Master tone against the Essential render | Try a +3 dB then +1.5 dB wide side bell at 2.8 kHz on the Master; keep the strongest that passes the mono fold-down, correlation and negative-band gates. Provisional. |
| `set.glue.v1` | Active detector level of the source | Glue trial of a 1.4:1 linked compressor, kept when it removes 0.3 dB of maximum limiter reduction without adding 0.25 dB of attack loss. Off: it never passed on the corpus excerpts. |
| `set.compression.v3` | The onsets and the section level (centred 4 s detector power) of the mix; the drum interval and the bass body frequency from the stems | Linked 2:1 compressor, 6 dB knee, before peak control in each version of a source that does not arrive limited. The threshold is an offset above the section level, and the section make-up gives back the energy of the gain reduction over the same 4 s window. Styles: Gentle, 1 dB (`essential`); Punchy, 1.5 dB (`master`, `open`); Dense, 2 dB (`bold`). The attack of Punchy is the slowest of 10 to 100 ms that keeps 95% of the largest punch at 1.5 dB; Gentle takes twice it (at most 150 ms) and Dense half (at least 10 ms). The release comes from the drum interval (100 to 600 ms) and the detector high-pass from the bass body (40 to 150 Hz); without stems, 300 ms and the full band (D202). Provisional. |
| `set.bold-level.v1` | The audibility point of peak control on the prefix of `essential` (`set.audible-loudness.v3`) | `bold` targets 0.5 LU under the point, at most −6.5 LUFS and at least the base target; a source that arrives limited, or that has no point, keeps the base target. The other versions keep the base target (D203). Provisional. |
| `set.sub-lift.v1` | The mid regions (sub − bass, bass − body) of the first render of a chosen base candidate with bass shelves, and of the mix under its tone | The shelves take the shortfall of each region on top of their move, within the largest outcome of the pairs and 4 dB, and only rise; the second render is kept when it passes the gates of the first. A shortfall under 0.1 dB in both regions takes no second render. A request on a delivered version keeps the shelves of its parent and renders no lift (D204). Provisional. |
| `set.input-condition.v1` | Plateau share and plateau level spread of the source's 10 ms block peaks | A source with 4% or more of its blocks within 1 dB of its 99.9th percentile block peak, over 1 dB or more of level, arrives limited: no compressor, its loudness as the base target, no Denser version (D130). |
| `set.side-cut.v1` | Side and mid power of the source in the 2 to 8 kHz and 200 Hz to 8 kHz bands | When the upper side/mid exceeds the body side/mid by 0.5 dB or more, cut the side at 4 kHz (Q 0.7) until they match, by up to 3 dB. Never lift. Replaces the +3/+1.5 dB side-lift ladder. Stem focus side bells under 500 Hz are bounded to ±1.5 dB. Provisional (D128). |
| `set.widening.v5` | Whole-file correlation and P10 0.5 s window correlation of each stereo band of the mix after the compressor and peak control at the base target; the median, P75 and P90 widening of the album pairs of D177 in each band | Superseded by `set.widening.v6` (D206). Side gain allowance per band from 120 Hz: correlation drop ≤ 0.35 (gate 0.40), P10 window correlation ≥ 0.05, extra mono loss ≤ 1.5 dB. Master: one gain set by the tighter midrange band, at least 0, and the 120–500 Hz band at least the median widening of the pairs there (+4.7 dB, `width::LOW_MID_WIDENING_DB`) within its own allowance (D183); when the midrange gain is under 0.5 dB, the bands from 500 Hz up keep the width of the mix and the side cut of `set.side-cut.v1` stays; Bold: each band's allowance, at least 0, the 120–500 Hz band no wider than the larger of the band above and +4.7 dB; the band under 120 Hz keeps the width of the mix in both; each band with a side in the mix at most the P75 widening of the pairs in `master` (+4.2, +5.0, +2.7, +2.1, +2.2, +2.2 dB from the band under 120 Hz up, `width::PAIR_WIDENING_P75_DB`) and the P90 in `bold` (+4.7, +5.2, +3.2, +2.4, +2.4, +3.0 dB, `width::PAIR_WIDENING_P90_DB`), and a band with no side within the decorrelated-mid bounds of D141 (D184); at most +6 dB; zero-phase side filter after tone; the tone's own side/mid change of each band, predicted on the source spectrum with side lifts, mid cuts and stereo filters (`width::tone_width_db`), counts toward it. A band whose P10 window correlation is under 0.05 is neither widened nor cut: the width of the mix stands. Essential: none (D132). Provisional. |
| `set.open.v3` | The long-term spectrum of the mix (the sub, bass and body regions of); the stereo image after the compressor and peak control at the base target | Deliver `open` beside `essential`, `master` and `bold` (D178) where both bands under 500 Hz admit at least 1 dB of the Open widening (`width::OPEN_FIT_MINIMUM_DB`), the condition the album masters answered; otherwise no Open version, with the reason in `widening.open_withheld`: the tone of the mix with two stereo low shelves, +1.4 dB at 90 Hz and +1.2 dB at 300 Hz (the median sub-against-bass and bass-against-body moves of the eight album pairs of D177), each lift reduced where the mix already reaches the largest outcome of the pairs (+5.74 and +9.67 dB); the Open widening: `width::OPEN_PATTERN_DB` (+4.0, +4.7, +2.0, +1.5, +1.5, +1.3 dB from the band under 120 Hz up, the medians of the pairs) under each band's allowance, with the P10 window bound of every version (0, and +0.05 with the margin of the plan, D186), and no further than the pairs' largest outcome in the band (`width::OPEN_OUTCOME_MAXIMUM_DB`), at least 0, at most +6 dB; the base target (D179); no de-esser, no low-mid correction, no low-end hold. Fallbacks: three quarters and half of the tone, then half the widening. The stereo gates of the selection stand. Provisional. |
| `set.source-condition.v4` | The mix: side presence, codec low-pass, input condition, integrated loudness, anti-phase bands (D142), the mean of each channel; with stems, the stereo image of each stem per band | Report mono, the bandwidth, the input condition, a quiet input (−24 LUFS or less, `source_condition::QUIET_LUFS`), the anti-phase bands and the DC offset of each channel in dBFS, at least −200 dBFS (D187); report a stem as an anti-phase instrument group in a band when 50% or more of its active 0.5 s windows have a correlation under −0.25, its phase is beyond 135° with a resultant ≥ 0.3, and every other active stem is within 45° of in phase, with the time range of the negative windows. The report changes no processing (D175). Provisional. |
| `adjust.mono-safe.v1` | The anti-phase bands of D175 and the 0.5 s window correlations of the mix | Preset of a request on a version that carries no mono-safe cut: one zero-phase side cut per anti-phase band of D175 at the gain that brings the band's P10 window correlation to +0.05 (at most −6 dB), wet in each negative window and its neighbours (`width::mono_safe`, `treatment_render::apply_mono_safe`), at the full gain, three quarters and half; the description gives the bands, the share of the song and the mono loss before and after (D176). Withdrawn on 2026-10-12 (`adjust::MONO_SAFE_OFFERED` is false): on a phone speaker the movement of the cut was distracting (D176). |
| `adjust.streaming.v1` | The target and the ceiling of the parent; the normalization level and the true-peak limits of Spotify, Apple and AES TD1008 | Preset of a request. A version louder than −14 LUFS goes to −14 LUFS, and the ceiling is −1 dBTP. Every other stage of the parent stays. One candidate. Offered where the level or the ceiling moves by 0.5 LU or 0.5 dB or more (D197). |
| `adjust.club.v1` | The audibility point of the set (`adjust::Facts`, D131); the target and the ceiling of the parent | Preset of a request. The level is the point, moved by the change of the ceiling, less 0.5 LU. It is at most −6.5 LUFS and never under the parent. The ceiling is −1 dBTP, and the low end stays. Candidates: the full rise, three quarters and half. Each candidate stays at or under the audibility point of its own prefix (`selection.own_point`). Offered where the level rises by 0.5 LU or more (D197). |
| `adjust.mono.v1` | The stored source spectrum (mid and side power in 1/24-octave bands), the tone and the side gain of the parent | Preset of a request. In each band that the parent widens, the side gain and the decorrelated mid return to 0 dB. A zero-phase mid gain (`width::SideGain::mid_db`) gives the mid the side power that they added, so the stereo power of the band stays (`adjust::folded`). Candidates: the full fold, three quarters and half. Offered where the predicted mono sum of a band at matched loudness rises by 0.5 dB or more (D197). |
| `adjust.radio.v1` | EBU R 128 s3; the fold of `adjust.mono.v1` | Preset of a request: −23 LUFS, a ceiling of −1 dBTP and the fold of `adjust.mono.v1`. Candidates: the full fold, three quarters and half. Offered where the level, the ceiling or the mono sum of a band moves by 0.5 LU or 0.5 dB or more (D197). |
| `set.deesser.v4` | Sibilant frames of the vocal stem in which the stem has half or more of the mix's 5 to 10 kHz mid power and more of it than each of the drum, bass and other stems; the vocal share of each keyed frame; the 5 to 10 kHz mid power of the drum, bass and other stems; the stem's share of the mix power; 5 to 10 kHz and 1 to 4 kHz power of the mix's mid channel in the keyed frames | Each version of a set measures the excess of the 5 to 10 kHz power over the 1 to 4 kHz power of the mid in each keyed frame, after tone and widening. Default (the switch `set::Options::deesser` off, or the request field absent or null): the largest cut is 0 dB, so no frame is cut, as in `set.deesser.v3` (D181). With the switch on: the threshold is the reference excess, the 90% quantile of the excess of the keyed frames (`excess_quantiles_db[18]`, `deesser::THRESHOLD_QUANTILE`), plus the threshold of the plan (0 dB in a set). A keyed frame above the threshold gets a zero-phase 5 to 10 kHz cut of the mid: the excess over the threshold times the vocal share, by up to 3 dB (`deesser::MAXIMUM_CUT_DB`). A cut that removes more than 0.3 dB of the 5 to 10 kHz mid energy of the drum, bass and other stems over the recording halves the largest cut once, then cuts nothing (`deesser::OTHER_LOSS_LIMIT_DB`). A render from a delivered version uses the reference excess and the largest cut of that version and runs no stem check. Fewer than 20 keyed frames: no key (D133). A stem with less than 1% of the mix power: no key (D153). Provisional (D209). |
| `set.render-rate.v1` | Sample rate of the source | Render at 44.1 kHz for a source at 44.1, 88.2 or 176.4 kHz, and at 48 kHz for a source at 48, 96 or 192 kHz. A source at a higher rate is converted down by a factor of 2 or 4. Each filter, time constant, window and frequency band of the chain is set in hertz or seconds at the render rate; FFT and filter lengths stay in samples. The interpolator and the decimator of peak control have a cutoff of 21.75 kHz at each rate. Each delivered file is at the render rate, and a worker plan records it (D173). |
| `delivery.copy.v1` | PCM24 master at its render rate, the ceiling of its version, the requested sample rate and word length | Convert the rate, measure the peak estimate again, turn down a copy above the ceiling by the excess, then dither at the word length of the copy (D154). The web client asks for the render rate. |
| `set.presence-balance.v2` | Mid and side band powers of the source spectrum; ISO 226 at 75 phon; the moves and outcomes of the album pairs of D177 | The static tone of `master`, lead bells included, moves presence (2–5 kHz) against body (500 Hz–2 kHz) toward the equal-loudness-per-band balance (−3.30 dB), never past it and never down, by at most +0.94 dB, the P75 move of the album pairs; a mix darker than the darkest album master (−9.10 dB) moves up to that outcome, by at most 2 dB (`broad_tone::presence_move_maximum_db`). The lead presence bell takes no more than the bound leaves after the shelves and the widening; a stereo bell at 3162 Hz, Q 0.7, of up to 3 dB adds the rest. Master family (D134, D180). A device capture takes `set.capture-presence.v1`. Provisional. |
| `set.capture-presence.v1` | Presence against body of the source spectrum; ISO 226 at 75 phon; the darkest album master of D177; the source condition (D142, D146) and its device capture (D200) | For a device capture (mono, an anti-phase band, or a codec low-pass on a source that arrives at −24 LUFS or less and unlimited: `SourceCondition::device_capture`), the static tone of `master` moves presence (2–5 kHz) against body (500 Hz–2 kHz) up to the darkest album master (−9.10 dB) where the recording is darker, and otherwise by at most +0.94 dB, never past the equal-loudness balance (−3.30 dB) and never down; the presence balance bell and the summed static response of `master` go up to 6 dB. `bold` keeps its step of `set.vivid-steps.v2` within the bound of `master` plus 0.48 dB (D200). Provisional. |
| `set.capture-low-band.v1` | Sub (30–80 Hz) against bass (80–250 Hz) and bass against body (500 Hz–2 kHz) of the source spectrum; the outcome range of the album masters of D177; the source condition (D146) and its device capture (D201) | For a device capture (as `set.capture-presence.v1`) whose sub against bass lies outside +0.59 to +5.74 dB or whose bass against body lies outside +2.78 to +9.67 dB, stereo low shelves at 80 Hz and 250 Hz, the tops of the two regions, move the measure to the nearest end of the range, in either direction, solved together on the source spectrum, each by at most 4 dB and only toward its own end; a shelf under 0.5 dB is not proposed. The corners differ from those of the bass shelves (90 and 300 Hz), so `set.sub-lift.v1` and the name Full read only the bass shelves. A correction of `master` and `bold` with no tonal gate: the low-end hold counts the shelves as intended moves, and its gate checks each section of the held bands (D201). Provisional. |
| `set.limiter-release.v1` | Masked disturbance of peak control on the confirmation mixes (D127) | Use a 10 ms limiter release, 12 ms hold and 1 ms windows in every set version. Provisional. |
| `set.peak-tone-restoration.v4` | Band powers and K-weighted energy of the prefix before the compressor and of each delivered render in each 0.34 s window | Render every set version once, measure the tonal change that the compressor and peak control caused inside each window at the window's matched K-weighted level in each material band, and render again with up to six broad filters (±1.5 dB each) before the compressor that return the balance before the compressor; a second pass returns the rest. Each change shrinks by 0.1 dB before it is restored. A version starts from the restoring gains of `essential`, and a held version takes its passes in the renders of the low-end hold (D160, D163). Provisional. |
| `set.dense-bound.v3` | Base render's limiter maximum; rendered denser candidate's limiter and attack loss | Lower each denser target to the largest target within the base render's 8 dB limiter headroom (1.5 dB of maximum reduction per LU); keep a candidate only when maximum reduction ≤ 8 dB, mean ≤ 3 dB and P90 attack loss ≤ 2 dB more than the base. No passing candidate, no Denser version. |
| `set.candidate-ladder.v1` | Gate results of each rendered candidate | Render candidates strongest first and keep the first that passes its gates; a version with no passing candidate is a no-opportunity result. |
| `set.described-duplicate.v1` | Matched-level band powers, side/mid, mean limiter reduction and delivered level | Exclude a version when no described measure differs from an already delivered version by its description threshold. |
| `treatment.spatial-overlap.v1` | Source-bound aligned-stem auditory diagnostics and adjacent measured spatial sensitivities | Propose localized mid/side EQ, retaining unchanged and individual/combined alternatives for subsequent complete rendering. Provisional; used by the complete native set job when matching diagnostics are supplied. |
| `set.version-character.v1` | Region powers (low mids, presence, air, sub, bass) and the side/mid of six bands of the mix | Each version has a character: a move of each region and a width of each band. Full takes the moves of the single masters of the reference engineer, Essential smaller moves, Warm more low end and less air, Wide the tone of Full with 1 dB more width. The tone is solved on top of the stem focus; a following low-mid move only cuts; a tone that fails `Tone::validate` scales down in 10% steps. Every version has the tone, the shelves, the widening, the de-esser, the low-mid cut and the low-end hold (D206). |
| `set.widening.v6` | Correlation allowance of each band of the mix, the outcome room, and the width of the character of each version | Widen each band of each version by the width of its character, bounded by the correlation allowance and the outcome room, from 0 to the widening maximum. A band with no side widens through the decorrelated mid; band 0 of such a band stays mono (D206). |
| `set.audible-loudness.v6` | Total masked NMR and P90 attack loss against the mix of the peak control of the set and of each candidate's own chain | Every version, Bold included, sits at the audibility point, with a −6.5 LUFS cap. The point is the loudest output at which the total masked NMR is at most 0 dB and the P90 attack loss is at most 5.45 dB. A candidate over its own point at its drive renders again 0.03 LU under its own point (`selection.own_point`). Each candidate reports the NMR of its loudest 5% of frames (`selection.own_loud_nmr`). An album target (`album.levels.v2`) caps the base and the Bold targets (D206, D207, D208). |
| `album.order.v2` | Summaries of the mixes of an album: length, point, crest, width tier, treble side, tempo, key and second key | Score each order of ten tracks or fewer by eight rules: opener, early peak, level step, width step, key step, tempo step, closer and vinyl sides. Each track within 0.25 dB of the lowest crest counts as the densest. A key that leads its second key by less than 0.05 is not clear, and its step takes the relation of the lower cost. Width and tempo steps have low weights. For more tracks, improve the order by swaps of two tracks. Give the reasons and the step of each position (D207). |
| `album.levels.v2` | The points of the tracks in the running order, and the delivered loudness of the mastered tracks | Put each track at its point, and each mastered track at its delivered loudness. Then lower the louder of two adjacent tracks to 1 LU above the quieter, or 2.5 LU for a rise into the last track, by at most 1 LU under its point. A mastered track keeps its loudness. Each track masters at or under its target (D207). |

Each plan records the active rule IDs and their reasons.
Runtime rejection reasons identify the failed preservation or limiter constraint.

## Native source-led set

`set::render` replaces the finishing-only `pipeline::master_set` prototype. It uses
`pipeline::analyze_source` once and renders through the same `treatment_render::Renderer`
as the A–F listening-study command. It does not route versions through the older
single-winner finishing/width/compression controller. Those single-render rules below
remain isolated to the older single-master API; they do not select this set.

With usable aligned stems, test one shared, bounded all-role tonal foundation in
Essential and every supported focus family, at both source-derived densities.
Render an unchanged-tone fallback for comparison and record its complete-chain
measurements as `comparison-only`; it is not a separate delivered character
version. The existing vocal focus regions remain candidate family characters,
with the shared foundation included in their plans. If a focus
filter exceeds the combined tone bounds, reduce its gain; if it still cannot fit,
test the foundation alone for that family. The complete-render duplicate screen
decides which remaining versions to deliver. No family is added merely to meet
a version count.
No reference fit, generic correction family or generic width menu is added.
The optional existing auditory artifact is bound to the original file and its actual
decoded PCM at the render rate; unusable or stale supplied evidence fails explicitly.

`set.source-density.v2` uses the negative median active 50 ms source crest as a
starting loudness target, clamped to −16…−8 LUFS. A median crest of at least 6 dB,
plus a P90−P10 crest span of at least 1 dB or eight measured onsets, supports an
additional trial. Its step is half the P90−P10 crest span, bounded 0.5–3 LU,
with the target capped at −6.5 LUFS. This replaces the fixed +1.5 LU step.
This mapping is a provisional search
heuristic, not a physical equality between crest and LUFS or an established mastering
target. Each report records the measured basis, actual target and limiter timing.
The unchanged tonal/spatial path is eligible at both densities. No waveform processing
is added merely to satisfy a version quota.

`set.all-role-foundation.v1` measures 29 one-third-octave mid and side powers in
all four supplied estimates. A role is active above its P95 minus 25 dB level,
with at least 2% of total estimated power. A candidate needs two adjacent bands
each carrying at least 0.5% of that role's active excitation, ten seconds of
eligible frames, an estimated role-to-others power ratio no greater than 0 dB,
and at least 0.25 dB predicted improvement from a +3 dB mid or side probe in
both bands. Side probes below 200 Hz are excluded. The selected bell is +1.5 dB
with Q bounded to 0.5–2; one region per role is allowed, and regions in the same
domain closer than half an octave cannot stack. Its wet amount is the locally
eligible frame fraction in each two-second source window, smoothed across adjacent
windows. Windows whose stem sum differs from the source by more than −18 dB or
has cosine below 0.98 supply zero raw wet duty; adjacent-window smoothing can
carry a small transition amount across their edge. Estimates select controls;
only the original stereo mix is processed. This is offline, passage-driven
automation of individual EQ bands; it is not the faster detector/threshold
`dynamic_after_eq` processor, which the set planner still leaves empty. These
thresholds and stem-power proxies are provisional. A no-opportunity result
leaves the original tone eligible.

Each branch retains the study renderer's eight bounded loudness probes and 4× processing.
Under `set.peak-share.v3`, every version delivers at a −0.5 dBTP ceiling (D155). The limiter
holds the signal 1.5 dB (base) or 2.5 dB (denser) above that ceiling, and the
ceiling-relative soft clipper maps the remainder onto it. The limiter uses the
selected fast release, a 12 ms hold on recurring crests, a 3 ms cascaded lookahead
and no sustained stage. A post-reconstruction trim removes any remaining
reconstruction overshoot. Nonfinite PCM, changed length,
missing loudness and failed peak/delivery verification are technical faults. The old
source-preservation bounds are recorded as diagnostics with their actual limits;
they do not back off or veto the rendered treatment. No preference score is claimed.

Before retaining a version, compare its measured signature with every already
delivered version under `set.described-duplicate.v1`. The gain-matched residual
against each delivered version is reported as a diagnostic.

Per-version sinks persist and independently verify PCM before the next branch. They
can stop the job at progress/delivery boundaries. Native comparison loads only one
previous WAV at a time. One oversampled prefix and at most 128 MiB of shared probe
PCM use seekable scratch storage. Native hosts use temporary files.
Completed version audio is not accumulated by the set renderer.
Source, temporal, stereo and other measurements are shared; each output is measured.
Descriptions use final loudness, gain-matched band powers, side/mid balance and limiter
work, relative to Essential (or source for Essential). Each measured change is one
sentence. The two strongest band changes share one sentence, which names the matched
level once.

`set.result-name.v1` assigns display names after rendering, relative to Essential.
The trial record holds the name as `measured_name`.
A proposed centre-region lift must achieve at least +0.3 dB matched mid power in
that region; a proposed side-region lift must achieve at least +0.3 dB side/mid.
Only then name that region “More centre …” or “Broader …”. “Denser” requires at
least +0.15 dB measured mean limiter reduction. Otherwise use the strongest matched
whole-band change of at least 0.5 dB, a measured stereo change of at least 0.3 dB,
or a neutral variation name. These are provisional description thresholds, not
audibility claims. Essential keeps its anchor name; internal IDs remain independent
of display names. WAV/report names include both, with bounded UTF-8 display/source
prefixes so long track names or combined treatments cannot exceed filename limits.

The native v2 manifest, sidecars and progress records replace their v1 prototypes.
The native command refuses an existing directory, preserves a byte-identical original
copy and writes an audition page. Browser playback starts at matched loudness,
with unity-gain delivered playback available and position retained on switching.
 See D039.

### Broad tone, width, glue and selection

`set.master.v1` composes one set from fixed roles. Essential applies the
foundation, low-end mono and peak control. Master joins the joint broad-tone
proposal with the strongest stem focus proposal (the combined focus where it
exists) and tries the side-lift ladder first; its fallbacks are the tone
without a side lift, three quarters and half of the tone with gate minimums
scaled alike, then the broad moves without the stem focus. Denser Master
derives from the chosen Master candidate at the denser target. Master · Bold
is the chosen Master candidate with its broad-tone moves, the stereo bands,
scaled by 1.5, gated for mono compatibility only. The foundation, focus and
side-lift bands keep their measured gains, so a focus bell near the 6 dB
clamp cannot push the summed response past the 9 dB domain bound. Denser Essential renders
only when Master finds no opportunity. The described-duplicate screen still applies to every version.
Every gate compares the candidate with the Essential render at the same
density, because the limiter and clipper change loud windows and channel
balance in ways that a source comparison misreads.

`set.broad-tone.v3` derives every broad move from the recording itself.
The delivered gain is the smaller of the base target and −14 LUFS, the
streaming normalisation level, minus the source loudness. ISO 226:2003
equal-loudness contours give, for each frequency, the sound pressure change
that produces the same loudness increase as at 1 kHz when the listener hears
the master that much louder than the mix at an assumed 75 phon; the
compensation is that change minus the delivered gain. Its mean over 40–125 Hz
is a low shelf at 150 Hz and its mean over 6.3–12.5 kHz a high shelf at 6 kHz.
With usable stems, the lead role is the vocal when it is active for at least
10 s and 15% of the frames where any role is active; otherwise the role with
the highest mean share of 500 Hz–5 kHz leads when it is not drums, holds at
least 35% share, beats the runner-up by 1.5 dB, is active for 10 s and has an
energy centroid of at least 500 Hz over 250 Hz–5 kHz. The lead's centroid c
(1 kHz for the vocal, otherwise clamped 500 Hz–2 kHz) sets its regions: low
mid [c/4, c/2), body [c/2, 2c) and presence [2c, 5c). The lead's mean share
of total estimated power over its active frames is measured per region in dB.
A presence share below the body share proposes a presence bell (Q 0.7 at
3.2c) of the deficit; a low-mid share below the body share proposes a low-mid
bell (Q 0.7 at 0.35c) cut of the deficit. Each gate measures the bell's
stereo band against the band holding the centroid. Every move is capped at ±2 dB and dropped
below 0.5 dB; the summed response is scaled to at most 3 dB. A candidate passes
when each move's region difference changes against Essential at matched level
in the intended direction by 30% of its filter gain and at least 0.2 dB: bass
against body (20–120 Hz against 500 Hz–2 kHz), low mid against body (120–500 Hz
against 500 Hz–2 kHz), presence against body (2–6 kHz against 500 Hz–2 kHz)
and air against presence (12–20 kHz against 2–6 kHz). The candidates of the
bass version leave out the gates of the bands under 500 Hz, and the low-end
gate checks those bands (`set.bass-version.v4`). Mean limiter
reduction may rise by no more than 0.3 dB. The Master tone, with or without a
side lift, must add no more than 1.5 dB of mono fold-down loss in any band,
drop no whole-file band correlation by more than 0.35 and turn no band
negative; center-share and mono-share limits do not apply, because a mid or
side lift changes them by construction.

`set.low-end-mono.v1` applies a second-order Butterworth side high-pass at
100 Hz to every version when side power below 100 Hz is at least 2% of total
power there. `set.side-lift.v1` prepends +3 dB and +1.5 dB wide side bells
(Q 0.5 at 2.8 kHz) to the Master ladder; a side-lift candidate must raise mean
side/mid over 500 Hz–12 kHz by one third of its gain against Essential, add no
more than 1.5 dB of mono fold-down loss in any band, drop no whole-file band
correlation by more than 0.35, and turn no band negative.

Denser versions derive from the chosen base candidate, and their loudness
search starts from the base render's drive corrected for the target
difference. `set.glue.v1` defines a linked 1.4:1 compressor trial (3 dB knee,
30 ms attack, 300 ms release, at most 2 dB, threshold at the 70th percentile
of the active detector level) kept when it removes at least 0.3 dB of maximum
limiter reduction without adding more than 0.25 dB of P90 attack loss. It is
off: on every corpus excerpt the compressed signal needed more drive and the
limiter worked harder. `set.dense-bound.v1` requires maximum limiter reduction
of at most 10 dB; otherwise the target drops by 1 LU, to no less than 0.5 LU
above the base target. The final plain candidate has no gate.

`set.dense-bound.v3` estimates the base render's limiter headroom as 8 dB
minus its maximum reduction and allows 1.5 dB of maximum reduction per LU of
target above the base. A denser target beyond that headroom falls to the
largest target within it. A target less than 0.5 LU above the base is not
tried, and targets closer than 0.25 LU are tried once.

`set.peak-tone-restoration.v4` applies to every set version. Broadband peak
control lowers the gain most at the largest peaks, and it lowers loud
passages more than quiet ones. The renderer measures the power of the
prefix before the compressor and of the delivered output in the six
`stereo::BAND_EDGES_HZ` bands, in windows of four 4096-sample blocks
(0.34 s). The change in a band inside one window is the power ratio minus the
change in the window's K-weighted power, which comes from the same windowed
blocks. The change of the band is the mean
over the windows, weighted by the band's prefix power in each window. Windows
more than 40 dB under the loudest prefix window are skipped. A level change
between passages therefore does not enter the change. Changes are rounded to
0.0001 dB and power shares to 1e-6, so native and WASM builds make the same
decisions. A band is material when it holds at least 1% of the prefix's
K-weighted power. Each change shrinks toward
zero by 0.1 dB, so a change of 0.1 dB or less is left alone and the
correction is continuous in the change. One filter per material band is
solved so that its mean log-frequency response equals the shrunk change with
the opposite sign: a low shelf at 120 Hz, bells at 245 Hz, 1 kHz, 3.46 kHz
and 8.49 kHz, and a high shelf at 12 kHz. Gains are bounded at ±1.5 dB, and
filters under 0.05 dB are dropped. With no filter left, the first render is
delivered. The filters apply after the window measure of the prefix and
before the compressor. A version starts from the restoring gains of `essential`.
The loudness search of a pass starts from the drive of the render before,
corrected by the prefix loudness change. A version with the low-end hold
takes its passes in the renders of the hold (D163). The render reports the measured change, the filters
and the delivered change. Gates and descriptions measure the restored render.

`set.described-duplicate.v1` replaces the −40 dB residual screen. A version is
a duplicate of a delivered version when every matched-level stereo band differs
by less than 0.4 dB, side/mid by less than 0.3 dB, mean limiter reduction by
less than 0.15 dB and delivered level by less than 0.3 LU. The residual is
still reported as a diagnostic.

### Source-specific strength, recovery and passages

Raw aligned estimates supplied with the auditory region hypotheses enable
`set.source-refinement.v1`. Source, stem and reconstructed-sum PCM fingerprints are
bound together. The validated resampled sum is reused; only the vocal needs another
conversion, and coherent accompaniment is the sum minus vocal (subject to float
rounding). No estimated stem or reconstructed sum is substituted for output audio.

`set.focus-strength.v1` applies the actual peak EQ followed by a bandpass at its
frequency/Q to the vocal and coherent accompaniment estimates. It measures regional
mid-plus-side power over two-second windows for gains 0…6 dB in 0.75 dB steps.
Eligible windows need reliable local reconstruction (residual ≤−18 dB, cosine ≥0.98),
active vocal, regional vocal power above 0.1% of full vocal power, and regional
vocal/accompaniment ratio ≤0 dB. Activity requires vocal power above P95−25 dB and
above 2% of vocal-plus-accompaniment power. At least ten eligible seconds are required.

When the maximum median ratio improvement is at least 0.1 dB, select the first
interpolated gain reaching 80% of that measured maximum and remeasure that actual
filter. Otherwise retain the study strength with a reason and the available probe
results. This is a bounded engineering response criterion, not a perceptual optimum.
These predictions precede passage blending, combined filters and limiting. Combined
gains scale together in 5% steps if needed to fit the existing 9 dB response bound;
the applied specification and scale are recorded separately from individual probes.

`set.stem-recovery.v1` requires usable stems and at least sixteen measured drum
decays. Convert the median six-dB decay to an exponential time constant by dividing
by `6 × ln(10) / 20`. Bound it to 25–160 ms, at least two cycles of the measured bass
body and at most half the median accepted drum-event interval. The bass-period
floor takes precedence if those two bounds conflict. Without reliable timing,
retain the study's 50 ms. This mapping remains a provisional mastering hypothesis.
The earlier 35–80 ms stem heuristic remains isolated to the older single-master
path; it is not the timing selected by the refined native set.

`set.focus-passages.v1` measures each two-second window separately. Unsupported or
locally unreliable windows request zero focus contribution. Supported windows need
at least 0.05 dB positive actual-filter response; their wet fraction is the negative
regional vocal/accompaniment ratio divided by its eligible P75 deficit (floor 1 dB),
clamped 0…1. Enable movement only when wet-fraction P90−P10 is at least 0.2.
Otherwise retain a static filter. Control points are at window centres and endpoints,
with raised-cosine interpolation. Recursive filter state runs continuously through
dry passages. Interpolation transitions across windows; these are slow mastering
movements, not sample-accurate vocal gates or musical section labels.

The renderer applies these envelopes to the original mix and includes every control
point in its shared-prefix identity. Names still follow achieved output measurements;
descriptions identify passage-varying focus where applied. The complete source
refinement is included in each delivered sidecar and the shared set evidence.
Elapsed work is reported separately from deterministic DSP evidence in native
progress, per-version timing and job preparation timing. The implementation
update of D040 gives the validation.

## Tonal and spatial comparison

`finishing.selection.4` measures center and mono energy as a share of each active band's total energy.
An intended linked EQ cut therefore does not count directly as center-image damage.
Band powers come from the same baseline-selected time windows, including candidate values that cross its independent activity floor.
Absolute zero/missing powers still fail coverage. Correlation, balance, bass-width, spatial variation, limiter workload, loudness, attack and crest checks remain.
The maximum absolute band-level change after global loudness matching is 1.5 dB.
The width selector keeps its existing absolute mono/center checks. None of these bounds establish sound quality.

## Supplied stem evidence

The engine runs no separator. The native CLI accepts `--stems DIRECTORY` with aligned, unnormalized `drums.wav`, `bass.wav`, `other.wav`, and `vocals.wav` from stemsplits.
MASTER uses four aligned stem estimates, including for instrumental tracks. Cloud mode uploads completed library stems or has the stemsplits AWS worker create them from the source before planning. Local-only mode requires four completed library stems. A partial or failed library split must be resolved before local mastering. Separation stays outside this engine.

All stems must share frame count and sample rate; their duration must match the original within one frame at the render rate after rate-conversion rounding.
The engine converts the stems to the render rate of the original for the masking measures, the de-esser key, the low-mid key and the refinement.
The reconstructed sum must have residual energy at least 18 dB below the source and source cosine similarity of at least 0.98.
These are coarse integrity checks, not proof of instrument isolation. Mismatches remain in the report and supply no correction candidate.
Evidence records exact float-PCM hashes of the source and all four estimates. A plan refuses evidence for a different source.

Analysis uses 8192-point stereo-power STFT, hop 4096 at the stem rate, with 29 one-third-octave bands from 25 Hz to approximately 20.3 kHz.
Vocal activity requires power above the vocal P95 minus 25 dB and above 2% of total estimated-stem power; at least three seconds must remain for vocal proposals. Aligned instrumental estimates can still inform bass control and limiter recovery.
Power shares are shares of estimates, not probabilities or additive source contributions.

The candidate locates the strongest vocal-energy band between 5 and 11 kHz whose estimated vocal power share is at least 50%, vocal relative-band P95 is above −20 dB, and P95-minus-median exceeds 6 dB.
The original mix must independently trigger the existing intermittent-excess rule at that center, Q 1.4, with 100 active 10 ms blocks. Stem-located proposals do not require sparse full-mix onset detection.
Vocal dynamic reduction is capped at `3 × intensity` dB (2.4 dB at the browser default). The relative-band detector retains 30 ms power smoothing, 20 ms attack and 120 ms release.

The bass trial locates the strongest estimated bass-energy band from 35–200 Hz with at least 50% bass power share across all activity, Q 1. The original mix needs at least 100 active blocks, band-level P95 above −45 dBFS and P95-minus-median at least 4 dB. Its absolute-band-power threshold is `max(P95 − 4, median + 1)` dBFS, slope 0.5, reduction capped at `2.5 × intensity` dB (2 dB default). It uses 5 ms power smoothing, 5 ms attack and 80 ms release; quiet band activity does not trigger merely because other instruments stop.

Drum timing uses 10 ms powers, peaks above P95 − 15 dB, at least a 3 dB rise over the preceding 80 ms minimum, 100 ms between peaks, and a subsequent 6 dB decay within 300 ms. At least 16 decays are required. Half the median decay sets recovery, with a two-bass-cycle floor and 35–80 ms bounds.

These trials have no static cut, extra color, clipping or section gain. Combined bands are tested first, then isolated bands, then generic proposals. Complete-chain matched comparison remains required. If a passing stage frees headroom, a final drive search targets the original request with all source-preservation and limiter constraints. The recovery attempt is recorded separately; it does not claim the same dynamics as the quieter baseline.
Instrument bleed remains possible, and processing the stereo mix also affects other instruments in that band.
Body/presence measurements do not automatically trigger boosts or cuts: energy location alone does not establish muffling or masking.

## Character filters

Removed (D137). The set path has no character control.

All character filters use Q = 1/√2.

| Character | Filters |
|---|---|
| Clean | No character EQ; automatic side filtering remains eligible |
| Warm | +0.6 dB low shelf at 160 Hz; −1 dB high shelf at 6 kHz |
| Bright, vocals | −0.35 dB low shelf at 160 Hz; +0.25 dB bell at 3.2 kHz; +0.6 dB high shelf at 8 kHz |
| Bright, instrumental | −0.35 dB low shelf at 160 Hz; +0.5 dB bell at 3.2 kHz; +1 dB high shelf at 8 kHz |

These settings define initial user-requested color, not automatic mix repair.
The finishing trial adds eligible harmonic shaping independently from these filters.
Overlapping responses can sum.
Vocal mode is a conservative setting, not source recognition or a de-esser.

## Temporal analysis

The analyzer retains stereo power in nonoverlapping 10 ms blocks.
It calculates crest factor over complete 50 ms groups.
The activity threshold is 40 dB below maximum block power, with an absolute power floor of 10⁻¹⁰.

An onset requires a 5 dB power rise over the preceding 50 ms average.
Its current block power must exceed the activity floor and 10⁻⁶.
The detector leaves at least 100 ms between events.
It excludes events without sufficient preceding and following samples.

Attack contrast compares the first 30 ms peak with RMS energy from 50 to 200 ms after the detected onset.
The renderer measures the same input-selected events after processing.
It does not redetect output events, which could omit damaged attacks.
At least eight events are required before this metric can reject a candidate.

Fixed 12-second windows supply RMS levels and crest factors.
An active window requires at least three seconds and sufficient input energy.
The same input-selected windows define the output comparison.
These windows do not identify musical sections.

## Preservation limits

Let `p` be punch and `i` be intensity, each between zero and one.
The set reports these limits as diagnostics with `p` = 0.7 and `i` = 0.8 (`pipeline::diagnostic_constraints`).

| Check | Limit |
|---|---|
| Maximum limiter reduction | `12 × i × (1 − 0.2 × p)` dB |
| Whole-file mean limiter reduction | `4.5 − 4 × p` dB |
| 90th-percentile 50 ms crest loss | `3.7 − 3 × p` dB |
| 90th-percentile attack contrast loss | `8.4 − 7.8 × p` dB |
| Loss of 12-second-window P90–P10 level contrast | `2.4 − 2 × p` dB |
| P90–P10 spread of per-window level changes | `2.9 − 2.2 × p` dB |
| Oversampled frames above 1 dB limiter reduction | `clamp(125 − 100 × p, 25, 95)` percent |

The browser starting intent allows 8.64 dB peak reduction, 2.5 dB mean reduction,
4.5 dB attack loss, 2.2 dB crest loss, 1.4 dB window-contrast loss,
1.8 dB window-change spread, and 75% of frames over 1 dB reduction.
Maximum punch retains the previous 0.6 dB attack and 0.7 dB crest allowances.
The larger search budget separates occasional peak control from sustained workload;
it is not a claim that this much processing is always musically acceptable.

The measurements use percentiles and can miss isolated damage.
They do not measure perceived vocal clarity, masking, distortion, or preference.
Increasing loudness must not bypass these checks.

## Rendering

The outer controller compares a base chain with eligible finishing candidates at similar achieved loudness.
The base chain uses the stages below. Finishing candidates add conditional stages.
Candidate order is combined, correction, character, then peak control; absent families and duplicate combined candidates are omitted.
The controller stops at the first passing candidate and records all remaining alternatives as unevaluated.
Every candidate compares with the same retained base result. Preservation bounds remain unchanged.

Each pass applies character EQ, optional side filters, eligible compressor comparison, 4× oversampling, and stereo-linked limiting. Under `compression.rules.5`, fresh compressor trials use the current material and limiter workload. Full-scale samples and candidate flat-top runs remain reported indicators; neither categorically forbids a rendered comparison.
Source indicators no longer impose a separate 1.5 dB peak-reduction cap.
The intensity-and-punch budget and all measured preservation checks remain active.
The report separates source eligibility from measured limiter-workload opportunity under the `mean-limiter-relief` objective.
Compression trials target the bypass master's achieved loudness and must reduce limiter workload within preservation bounds.
Benefit compares with a re-rendered compression-bypass reference at the candidate's measured final level, within 0.02 LU.
This reference retains the candidate's width filters. Original-bypass preservation checks remain active.
Separate compressor and limiter maximum reductions must sum to no more than the existing reduction budget.

The internal ceiling is 0.1 dB below the requested ceiling.
The limiter uses the selected release, a 12 ms hold on recurring crests and a 3 ms
cascaded lookahead.
An eligible clipping trial raises the limiter ceiling by its budget and soft-clips the
remainder onto the internal ceiling.
Its lookahead envelope requires complete offline data.

The first candidate uses at most eight loudness probes, measured after final peak trim.
The search targets 0.05 LU accuracy, with steps no larger than 1.5 dB.
It retains the closest measured output, then applies the existing preservation checks.
The drive bound reserves the measured compressor maximum before allocating the remaining reduction budget to limiting.
With finishing active, it also reserves color reduction. Final checks include clipping in the combined reduction budget.
Up to nine further candidates reduce drive in 1.5 dB steps from the measured first candidate.
After the first pass, three midpoint probes refine the interval between the passing and last rejected drives.
The report identifies the selected accepted candidate even when a later midpoint fails.
Each output receives reconstructed-peak verification and attenuation when required.
The final attenuation trims any reconstruction overshoot to the requested ceiling.
Search termination does not establish global target infeasibility.

The loudest passing limiter candidate from that bounded search becomes the pass output.
If all ten coarse candidates fail, that pass uses constant gain on the original waveform.
That result omits character filters, side filters, and compression.

The imaging controller uses the compression-bypass result with unchanged width as its comparison.
It then evaluates eligible side-filter proposals against that result.
It compares center energy and mono-band power at matched integrated loudness.
It also checks channel balance, correlation, bass width, spatial variation, loudness difference, and limiter workload.

The controller selects the smallest tested adjustment that passes all imaging checks.
If none passes, it retains the exact unchanged-width result.
If the unchanged-width pass required gain-only output, the controller skips imaging trials.
The final report separates the mastering status from the imaging outcome.
It also reports compression selection and each trial's effective loudness target.


The outer comparison retains exact base-chain audio when all evaluated finishing alternatives fail.
Delivery writes new files, applies integer dither once, decodes the result, and verifies its measurements.

## Evaluation references

Evaluation reference policy (D063, 2026-09-26): a commercial online mastering
service supplies integrated
loudness comparisons only. Its dynamics, compression, transients, tone, stereo
balance and sound are not target behaviour or evidence of desired processing.
Track/revision pairing must be explicit. Source-relative measurements, completed
renders and listening supply the evidence for dynamics decisions. Historical
reference-dynamics observations are not active evaluation guidance.

## Tutorial incorporation

Future tutorial notes must separate an engineer's observation from our interpretation and implementation.
Record the material, processing context, control interaction, exceptions, and source timestamp.
Add a counterexample and a regression test before promoting advice into a default rule.
Retain the original attribution when the implementation changes.

## Explicit treatment studies

The native `source_treatments::propose` stage imports `mastering.auditory-analysis.1`
source diagnostics. The caller supplies the actual original file hash, frame count
and native rate; all must match the report. The report must record unchanged sources,
the expected 64-band grid and valid stem provenance. Stem reconstruction must meet
the existing −18 dB residual / 0.98 cosine checks, and source-rate conversion residual
must be at most −35 dB. Missing stem diagnostics retain only unchanged processing;
malformed or mismatched evidence fails explicitly.

Without an imported artifact, native and WASM set jobs with four aligned stems
measure the same 64-filter auditory grid locally at the stem rate. The analyzer
uses a 4096-point periodic Hann window, 1024-frame hop, roex power filters,
vocal P95-minus-25 dB and 2% activity gates, and local mid/side +3 dB ratio
probes. It applies the same `treatment.spatial-overlap.v1` proposal policy below.
The stem sum must independently pass the −18 dB residual and 0.98 cosine
source-match checks; invalid estimates cannot produce focus proposals. No stem
audio is rendered into an output master.

The first proposal family carries over the study's provisional thresholds: 200–4000 Hz,
at least ten eligible seconds, at least 0.5% vocal excitation share, median estimated
vocal/accompaniment ratio at most 0 dB, and at least 0.25 dB positive local ratio
response to a +3 dB mid or side probe across two adjacent auditory filters. Invalid
or ineligible bands break regions. Centres are excitation-weighted geometric means;
bell Q is centre divided by the measured centre span, clamped to 0.5–2. The proposed
gain is +3 dB, a carried-over experiment setting, not a validated optimal amount.

Retain unchanged tone, each eligible region, and a combined treatment when the
existing `Tone` response/filter bounds permit it. A failed combined bound does not
remove individual proposals. No fixed proposal count, reference fit, or inverse
research control is added. The output identifies the rule, measurements and analysis
hash. Predictions are not completed-result descriptions or acceptance decisions;
complete-chain rendering and duplicate evaluation occur in `set::render`; listening calibration remains open.
 See D038.

The vocal-focused proposer above remains a separate candidate-family rule.
The shared foundation covers bass, drums, other and vocals across the measured
29-band range, including regions that the vocal rule cannot propose. The set's
worker plan still has no compressor or `dynamic_after_eq` alternatives.
The track-f and track-e audit motivated this extension; its
diagnostic probes are not automatic presets.
The completed 19-recording corpus reassessment extends
that evidence to 94 automatic masters and 19 level-only comparisons. Its all-role,
full-range diagnostic probes are evaluation tools; production proposal and density
rules remain the implementation described above.

The 2026-09-27 all-role local treatment study measured
every role and the useful frequency range, then rendered 56 local candidates,
eight static controls, seven bass-body trials and five outside-pass candidates.
The study preceded `set.all-role-foundation.v1`; it remains an independent
measurement and listening reference. Processing a band is conditional even when
its measurements are always included. Its bass-headroom trial threshold remains
outside the production rule.

Schema-2 listening jobs can render bounded stereo/mid/side EQ and insertion-point-calibrated dynamic bands through the existing complete chain. Every specified experiment is retained, source/renderer/output fingerprints and delivery checks are mandatory, and source-budget violations are reported. This does not alter automatic selection or approve the listening candidates for production. See [D033](DECISIONS.md) and treatment results.

The explicit treatment runner batches jobs by source and groups identical processing prefixes.
It reuses exact source PCM, temporal/loudness/stereo analysis, the finished oversampled prefix,
and matching final-drive probes. A prefix key includes finishing, tone, dynamic-band and
compressor settings; a probe key additionally includes exact drive bits, ceiling, clip
budget and limiter timing. Only one prefix is held at a time, and cached probe audio is capped at 128 MiB per
renderer process. This cache budget counts stored PCM bytes, rather than resident memory.
The best search candidate also uses scratch storage until selection.
Each job still renders or reuses its exact samples and independently
remeasures preservation, output, stereo and decoded delivery. Existing verified outputs are
never replaced. Reuse changes execution time, not treatment rules or acceptance policy.

## Block processing and audio I/O

D042 changes allocation and I/O boundaries. It preserves the processing rules.
The treatment renderer streams one forward limiter state across blocks. Its
lookahead lag holds a short ring of driven frames. Applied gains retain `f32` rounding.
Downsampling retains the complete filter overlap and the original summation order.

SoundKit decodes and encodes WAV in bounded byte and frame blocks.
The engine retains source precision, deterministic dither order, and immutable outputs.
D043 stores prepared prefixes and search candidates in seekable scratch storage.
Prefix generation and optional global-peak colour processing use bounded blocks.
SoundKit converts temporary PCM to and from exact float32 little-endian bytes.
Native files are temporary; WASM defaults to memory unless the host supplies a storage adapter.

Native stem analysis loads each role once in drums, bass, other, vocals order.
It preserves the original summation order, hashes, measurements, and reconstruction checks.
Only the vocal estimate remains for refinement. The reconstructed sum becomes accompaniment in place.
Equal-rate resampling consumes an owned buffer without cloning it.

Source PCM, a current output, and decoded delivery/comparison buffers still scale with duration.
Stem reconstruction and resampling also retain full-track buffers.


## Browser mastering sets

The MASTER screen is nested under `/music/<track-id>/master` in the Music library. The public splash wordmark opens `/music`; Art is `/art`, and the combined record shelf and deck are under `/artmusic`. The removed `/app`, `/tracks`, `/lightroom`, and `/mix` routes do not resolve to the app. The record shelf is one column with records below its navigation. MASTER follows the Account processing preference without displaying a processing-mode banner or Account shortcut.

The local MASTER screen requests `set::render` through the WASM bridge. It requires four completed library stems; with none it asks the listener to create them in STEMS, and with an in-progress split it waits for all four imports. The worker reopens its job directory from cloneable library, track and job identifiers, decodes SoundKit source and the four saved aligned stem estimates into float32 OPFS input files, and injects OPFS scratch handles into the renderer. The browser does not pre-reject stems by raw sample rate or frame count: Rust validates alignment after resampling, including a one-frame duration tolerance, four-stem consistency, and reconstruction quality. The renderer always processes the original stereo mix. The original comparison copy is delivered as a complete float32 WAV before stem analysis. Each accepted version is independently delivered as PCM24, reread and verified before it becomes visible. Native and WASM derive the PCM24 dither seed from the verified decoded source PCM hash and version ID, independent of the encoded source container. Existing outputs are not overwritten. Matched listening attenuates versions to the lowest integrated loudness at or below −18 LUFS; delivered level applies unity gain. This listening gain does not alter downloads.

When Account's local-only switch is off, a signed-in browser mints a short-lived Zeroth token with audience `MSTR` from the Rust identity Worker and sends it to `mstr.yl.vin`. API Gateway calls Zeroth's Rust AWS authorizer before the Rust mastering control Lambda can create or read an owner-bound job. The browser sends the required stored SoundKit v2 FLAC source (`lossless.sk2`) through a create-only signed upload. If four saved stems are ready, it also sends their existing SoundKit v2 Opus (`stream.sk2`) objects in fixed drums, bass, other, vocals order. Otherwise the stemsplits whole-track AWS worker reconstructs four estimates from the uploaded original using its shared SplitSession and segment service. The native ARM64 planner requires all four Opus estimates, validates their alignment and reconstruction, and records a source-bound family plan and an original comparison WAV. Each family Lambda renders the original stereo source and validates its decoded frame count and fingerprint against the plan. The last completed family invokes finalization independently of browser polling. Finalization reads unquantized candidates and delivered PCM24 WAVs in plan order, applies `set::matched_residual_db` at −40 dB and `set::measured_label`/`set::describe_measured`, and publishes accepted versions. The browser verifies each downloaded SHA-256 and stores each WAV as a new local file. Local-only mode runs the full set in WASM without upload.

The cloud Essential family exposes verified Essential first, then Denser when
its measured duplicate rule retains it, through source-bound markers in the
owner-bound status response. Each preview includes an absolute-scale waveform, the selected treatment spec, and a 384-bin limiter gain-reduction trace measured during that render. The limiter trace reports mean and maximum reduction in dB for each output-aligned time bin; it describes the limiter's gain only. A separate browser analysis derives a frequency-coloured waveform and 128 time-aligned, 96-band spectra from each saved WAV, including earlier masters. The original and selected spectra are shown together at the playhead, with the selected static EQ response from the saved spec. The limiter view overlays the measured reduction trace on the output waveform when the trace exists. Browser analysis does not change the mastering render or preview protocol. The browser verifies and stores each PCM24 WAV
before LISTEN. The final manifest must match every early WAV hash before the
browser enables that version's report and delivery actions. Other treatment
families render independently and finish through the normal finalizer.

Every new library import writes both a SoundKit FLAC preservation copy of the decoded source and the 192 kb/s Opus playback copy. Local and cloud MASTER require FLAC, including when the supplied source was AAC or MP3; the LOSSLESS badge remains tied to SoundKit's detected source codec, not the stored copy. Older Opus-only imports require reimporting from the original file before MASTER or source export. The MSTR API accepts only FLAC and has no codec-choice field. Cloud source objects may be up to 200 MB to cover five minutes of supported 96 kHz stereo 24-bit FLAC.

## Requests on a delivered version

A request is a number of steps on each of eight controls of one delivered version (`adjust.steps.v1`, D171, D172).

- The planner stores its facts in the worker plan and the manifest (`adjust.facts.v1`): the source spectrum in 1/24-octave bands, the audibility point of the set, the band of the source and the signature of the mix. Each version record stores its signature and the measures of the controls. Each path builds the candidates of a request from these stored facts.
- A tone step is 0.5 dB of its measure at matched loudness. Bass is 40–250 Hz against 500 Hz–2 kHz, Presence is 2–5 kHz against 500 Hz–2 kHz, and Air is 8–16 kHz against 2–5 kHz.
- The builder makes candidates at 100%, 75% and 50% of the steps. It predicts each tone step on the tone with the hold and the low-mid correction of the parent. It leaves out a share whose tone exceeds 9 dB in a domain with the hold of the parent added.
- A Sibilance step changes the mean de-esser cut over the keyed frames by 0.5 dB, less cut for a step up. The step raises the threshold and lowers the largest cut by the same shift. The threshold stays from 12 dB under to 6 dB over the reference excess (`deesser::THRESHOLD_RANGE_DB`), and the largest cut stays at or under 12 dB (`deesser::CUT_LIMIT_DB`). A step up larger than the remaining mean cut takes the cut to 0 dB, with the threshold at most 6 dB over the reference excess. A step up on a version without a cut is outside the range (`set.deesser.v4`, D209). The builder solves the shift from the excess quantiles, the vocal share between neighbouring quantiles and the reference excess of the parent's render (`deesser::Stats`).
- The range of a version (`adjust::range`) gives, for each control and step, the first share that the builder offers, or none. The panel shows it before a request. A request with no share is a result with the reason `outside-range`, not an error.
- The final render solves the low-end hold and the peak-tone restoration again. The hard gates are the ceiling and the delivery verification, the loudness target, the mono fold-down, correlation and negative-band gates, and the low-end hold. A Club candidate also stays at or under the audibility point of its own prefix (`selection.own_point`, D197).
- The finalizer (`adjust::finalize`) gives the warnings `loudness-above-audibility-point` and `limiter-workload`, and the measured change of each control at matched loudness.
- A request is a duplicate when its signature duplicates the mix, or duplicates a version at a loudness within 0.5 LU and no control measure, compressor reduction, de-esser cut or ceiling differs by 0.5 dB or more. A duplicate is not delivered.
- The name is the parent's name and the largest measured change of a requested control, or the largest requested step. A taken name gets a number.
- The preview renders 4 s spans with 4.5 s of context before and 2.5 s after, at the render rate of the set (`excerpt`). A span starts on a compressor section block of 10 ms, and the de-esser places its masking frames at their positions in the recording. With no steps, a span equals the delivered render apart from the last bit. The preview keeps the restoring gains and the true-peak trim of the parent. It predicts the hold of the final render from the sections that the parent stores (`HoldSections`): each section's measured change without the parent's hold bells, plus the predicted change of the held bands from the request, goes through the two steps of the hold (`low_end::next`), as in the final render. Between the steps, the preview gives the rise of the first step from the stored spectrum after the tone of the version (`low_end::rise`). A bass shelf makes the bass a larger share of the loudness, so the loudness match takes more of each bell's lift than the source spectrum predicts. It predicts the drive from the change of the prefix loudness in the stored spectrum and from the change of the compressor reduction.
- A request carries steps or a preset (`adjust.mono-safe.v1`, `adjust.streaming.v1`, `adjust.club.v1`, `adjust.radio.v1`, `adjust.mono.v1`). A medium preset keeps the level and the ceiling of its medium in each candidate. `adjust::range` offers a medium preset where its predicted change reaches 0.5 dB or 0.5 LU in a measure that serves the medium (`adjust::offers`). The measure is the level or the ceiling for Streaming, the level for Club, and the mono sum of a band for Mono. Radio takes each of them. The name is the parent's name and the preset's name. The result gives `medium` (D197):
  - the delivered level, ceiling and true peak, with those of the parent;
  - the change of the PLR, of each band at matched loudness, of the width and of the work of peak control;
  - with the fold, the predicted change of the mono sum of each band, and the measured mono fold-down loss of the mix and of the result.
- The cloud renders a request in the family Lambda while it keeps the job (one day). `ADJUST_ACCESS` and `ADJUST_LIMIT` set who can request and how many adjusted versions a set holds. After one day, the browser engine renders a request (`adjust_version`) for a track of up to five minutes.

