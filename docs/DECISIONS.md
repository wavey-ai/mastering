# Mastering decision log

Log start: 2026-09-24.

This log records design choices, their reasons, alternatives, evidence, and subsequent changes.
Initial entries record the existing prototype and the latest review proposals.
Their record date does not establish when the implementation changed.

The [rule ledger](RULES.md) describes active processing.
The task list contains implementation tasks and open questions.
Each render's JSON report records decisions for that recording.
This log records decisions about the engine itself.

## Record procedure

1. Add an entry for each material DSP, controller, threshold, interface, delivery, or evaluation decision.
2. Assign the next unused `Dnnn` identifier.
3. Record the date, decision status, implementation state, and origin.
4. Separate source advice, engineering interpretation, measurements, and listening results.
5. Link the relevant rule IDs, code, tests, research notes, and TODO section.
6. Record alternatives, known risks, and conditions for another review.
7. Append dated evidence and status changes to the entry.
8. Create a replacement entry when the decision changes materially.
9. Link both entries when one decision supersedes another.

Preserve earlier reasoning and failed results.
Mark factual corrections with their date and reason.
Keep raw transcripts, audio, credentials, and personal paths outside this log.
Use speaker roles, topic descriptions and timestamps for tutorial evidence; keep titles, speaker names and links in private records.
Identify measured audio and builds with hashes or linked measurement records.

## Status definitions

| Status | Meaning |
|---|---|
| Proposed | An option under consideration; approval and implementation are not implied. |
| Accepted | The chosen product or engineering direction; sound quality still requires evidence. |
| Provisional | A prototype choice that requires further validation. |
| Rejected | An option declined for the recorded reason. |
| Superseded | A later entry replaces this decision. |

Implementation state is separate from decision status.
A passing numerical test does not establish listener preference.
A tutorial example does not establish a universal processing rule.

## Index

| ID | Recorded | Subject | Status |
|---|---|---|---|
| [D001](#d001-model-free-automatic-processing) | 2026-09-24 | Model-free automatic processing | Accepted |
| [D002](#d002-current-processing-chain) | 2026-09-24 | Initial processing chain | Superseded by D007 |
| [D003](#d003-automatic-stereo-width) | 2026-09-24 | Automatic stereo width | Provisional |
| [D004](#d004-evidence-and-decision-reporting) | 2026-09-24 | Evidence and decision reporting | Accepted |
| [D005](#d005-separate-compressor-proposal) | 2026-09-24 | Separate compressor proposal | Superseded by D007 |
| [D006](#d006-section-dependent-processing-proposal) | 2026-09-24 | Section-dependent processing proposal | Proposed |
| [D007](#d007-compressor-experiment) | 2026-09-24 | Compressor experiment | Provisional |
| [D008](#d008-expressive-control-proposal) | 2026-09-24 | Expressive control proposal | Proposed |
| [D009](#d009-audio-science-scope) | 2026-09-24 | Audio science scope | Accepted |
| [D010](#d010-metric-led-development-phase) | 2026-09-24 | Metric-led development phase | Accepted |
| [D011](#d011-bounded-loudness-search) | 2026-09-24 | Bounded loudness search | Provisional |
| [D012](#d012-ylvin-tracks-integration) | 2026-09-24 | yl.vin Tracks integration | Accepted |
| [D013](#d013-basic-complete-stereo-chain) | 2026-09-24 | Basic complete stereo chain | Provisional |
| [D014](#d014-delivery-and-source-relative-album-pass) | 2026-09-24 | Delivery and source-relative album pass | Provisional |
| [D015](#d015-finishing-stage-family-alternatives) | 2026-09-24 | Finishing stage-family alternatives | Provisional |
| [D016](#d016-confirmation-corpus-audit) | 2026-09-24 | Confirmation corpus audit | Accepted |
| [D017](#d017-premix-corpus-and-native-rate-validation) | 2026-09-24 | Premix corpus and native-rate validation | Accepted |
| [D018](#d018-compressor-source-eligibility-and-opportunity-evidence) | 2026-09-24 | Compressor source eligibility and opportunity evidence | Provisional |
| [D019](#d019-level-matched-compression-benefit) | 2026-09-25 | Level-matched compression benefit | Provisional |
| [D020](#d020-defer-automatic-compression-for-initial-beta) | 2026-09-25 | Defer automatic compression for initial beta | Accepted |
| [D021](#d021-browser-render-and-report-boundary) | 2026-09-25 | Browser render and report boundary | Provisional |
| [D022](#d022-absolute-source-and-master-visual-measurements) | 2026-09-25 | Absolute source and master visual measurements | Accepted |
| [D023](#d023-source-peak-cap-and-preservation-search) | 2026-09-25 | Source peak cap and preservation search | Provisional |
| [D034](#d034-generate-masters-before-asking-for-preference) | 2026-09-25 | Generate masters before asking for preference | Accepted direction |
| [D035](#d035-first-native-source-reused-set-path) | 2026-09-25 | First native source-reused set path | Superseded by D039 |
| [D036](#d036-native-set-delivery-boundary) | 2026-09-25 | Native set delivery boundary | Superseded by D039 |
| [D037](#d037-reuse-identical-treatment-render-work) | 2026-09-25 | Reuse identical treatment render work | Provisional |
| [D038](#d038-bring-source-derived-treatment-proposals-into-the-native-engine) | 2026-09-25 | Native source-derived treatment proposals | Provisional |
| [D039](#d039-complete-a-native-set-job-using-the-af-treatment-renderer) | 2026-09-25 | Complete native treatment set | Implemented; calibration provisional |
| [D040](#d040-refine-treatment-amount-timing-and-passage-specific-evidence) | 2026-09-26 | Refine treatment amount, timing and passage-specific evidence | Implemented; calibration provisional |
| [D041](#d041-measure-job-resources-and-stage-alpha-deployment) | 2026-09-26 | Measure job resources and stage alpha deployment | Measurement implemented; deployment proposed |
| [D042](#d042-bounded-peak-control-and-soundkit-io) | 2026-09-26 | Bounded peak control and SoundKit I/O | Accepted |
| [D064](#d064-evaluate-all-role-full-range-local-treatment-trials) | 2026-09-27 | Evaluate all-role full-range local treatment trials | Accepted study; production rule proposed |
| [D065](#d065-integrate-verified-core-treatment-instead-of-role-trial-versions) | 2026-09-27 | Integrate verified core treatment instead of role trial versions | Proposed; evaluation in progress |

## D001: Model-free automatic processing

- Recorded: 2026-09-24.
- Origin: User requirements from the mastering discussion.
- Status: Accepted.
- Implementation: Rust runtime and basic intent controls exist; the explanatory interface remains planned.
- Decision: Use our deterministic DSP and explicit rules, without trained models or stem separation.
- Decision: Accept basic musical intent; select width automatically without a user width setting.
- Reason: The user requests automatic mastering with our code and a small set of inputs.
- Alternatives: Model-based mastering, required reference tracks, and required processor settings are outside the current product direction.
- Evidence: Repository instructions, [controls](../README.md#controls), and product requirements.
- Limits: These requirements do not prove superiority over AI mastering or human mastering.
- Review condition: An explicit user change to the product requirements.

## D002: Current processing chain

- Recorded: 2026-09-24.
- Origin: Code inspection; this entry records existing behavior.
- Status: Provisional.
- Implementation: Present in the working tree; this entry does not identify a published release.
- Decision: Retain the current chain as the comparison for further processing experiments.
- Chain: Character EQ, optional side EQ, 4× oversampling, gain, linked limiting, downsampling, and reconstructed-peak checks with trim.
- Reason: The existing renderer supplies an executable comparison with recorded processing limits and failures.
- Alternatives: Separate compression, adaptive limiting, dynamic EQ, saturation, and controlled clipping remain unimplemented.
- Evidence: [Render code](../src/pipeline.rs), [DSP](../src/dsp.rs), and [active rules](RULES.md).
- Rule context: `engineered-rules.2`; imaging policy `imaging.rules.1`.
- Limits: Character EQ uses fixed settings; limiter release uses one setting per recording.
- Limits: Genre selects a provisional loudness range; vocal context sets safeguards rather than detecting vocals.
- Verification: The recorded imaging run includes measurements and the previous 53-test result.
- Review condition: A new stage or controller change passes regression tests and receives comparative listening evidence.

This entry does not make every current filter, threshold, or processing order permanent.
The existing source-preservation checks remain provisional engineering choices.

### D002 updates

- 2026-09-24: Documentation-only change; runtime code remained unchanged during log creation.
- 2026-09-24: `cargo fmt --check`, Clippy with warnings denied, and all 53 release tests passed on the working tree.
- 2026-09-24: No new audio render or listening test occurred during log creation.
- 2026-09-24: D007 supersedes this chain description by adding optional compression. This entry retains the earlier comparison baseline.

## D003: Automatic stereo width

- Recorded: 2026-09-24.
- Origin: User width requirement and existing imaging implementation.
- Status: Provisional.
- Implementation: Bounded expansion and image retention exist; corrective narrowing and stereo synthesis do not.
- Decision: Adjust existing side information with small fixed-per-recording filters, then evaluate the complete mastering chain.
- Decision: Keep an unchanged-width master as the comparison and retain it when trials fail.
- Reason: Side changes can alter subsequent limiter gain, including the center signal.
- Alternatives: Unconditional widening, a fixed genre width target, and mono-to-stereo synthesis are outside the current policy.
- Evidence: Imaging policy, [imaging tests](../tests/imaging.rs), and confirmation-mix measurements.
- Limits: Eligibility and preservation checks establish processing bounds, not the artist's preferred width.
- Counterexample: A deliberately narrow mix can remain preferable after every widening check passes.
- Verification: The recorded run on the confirmation mixes passed numerical checks; controlled listening preference remains unestablished.
- Review condition: Matched-level listening contradicts the width objective or reveals center, bass, phase, or transition damage.

## D004: Evidence and decision reporting

- Recorded: 2026-09-24.
- Origin: User request for a decision log and an explanatory interface; existing repository evidence requirements.
- Status: Accepted.
- Implementation: Render reports and this design log exist; the explanatory interface remains planned.
- Decision: Record applied, bypassed, rejected, and unevaluated processing with its measured or supplied basis.
- Decision: Separate numerical correctness, preservation limits, achieved loudness, and listening preference.
- Reason: Each result must distinguish implemented behavior from a hypothesis or a tutorial example.
- Alternatives: Treating a passing metric, a louder result, or a named engineer's setting as proof of improvement.
- Evidence: Report fields, evaluation tasks, and the user's current log request.
- Limits: Reporting a reason does not validate that reason.
- Verification: Future material changes must link regression results and state whether listening tests occurred.
- Review condition: A result cannot be traced to its source, settings, policy, or comparison.

The future interface should explain automatic decisions before it offers optional overrides.
The user should not need to select width to obtain a master.

## D005: Separate compressor proposal

- Recorded: 2026-09-24.
- Origin: Assistant recommendation after the transcript and code review; not a user-approved priority change.
- Status: Proposed.
- Implementation: Not implemented.
- Proposal: Compare a bypassable stereo compressor with bass-sensitive detector options before final peak limiting.
- Reason: The current limiter provides peak control but no separate musical compression stage.
- Source: A fourth engineer, in a tutorial video on mastering for an immersive format, 13:14–13:43.
- Source context: He filters the compressor detector in an immersive-format demonstration; final stereo peak protection remains a separate engineering requirement.
- Counterexample: A third engineer starts without extra compression on an already-compressed mix; see the tutorial review.
- Alternatives: Compressor bypass, limiter refinement first, or source-aware EQ first.
- Evidence state: Tutorial interpretation and code inspection; no compressor experiment or listening result exists.
- Verification requirement: Stereo stability, bass-driven modulation, transient preservation, bypass behavior, and matched-loudness listening.
- Review condition: Evidence supports the stage and its priority over the alternatives.
- Work reference: Adaptive limiting and perceived density.

### D005 updates

- 2026-09-24: The user's continuation request was interpreted as proceeding with the recommended compressor experiment.
- 2026-09-24: D007 supersedes this proposal with a provisional implementation. Sound-quality validation remains open.

## D006: Section-dependent processing proposal

- Recorded: 2026-09-24.
- Origin: Assistant interpretation of the second transcript review.
- Status: Proposed.
- Implementation: Fixed-window measurements exist; section-dependent gain and tone automation do not.
- Proposal: Evaluate bounded gain and tone changes across detected signal changes, with explicit transition checks.
- Reason: Preserving every source level difference can conflict with the intended finished balance.
- Source: A fifth engineer, in a tutorial video on mastering a pop single, 00:12–02:29.
- Source context: He changes gain and processing across one song while retaining a perceived dynamic build.
- Source: A sixth engineer, in a tutorial video on mastering a song from start to finish, 30:36–31:10.
- Source context: The sixth engineer checks transitions so quieter-section changes do not remove the next section's impact.
- Alternatives: Retain fixed settings and current long-window preservation checks.
- Counterexamples: Intentional quiet passages, fades, sparse arrangements, and deliberate changes in width.
- Evidence state: Automatic transcripts, not auditioned examples or validated controller rules.
- Verification requirement: Transition continuity, retained musical contrast, artifact checks, and matched-level listening.
- Review condition: A bounded controller improves reserved recordings without flattening intentional changes.
- Work reference: Source-aware tone and dynamics.

### D006 updates

- 2026-09-24: D013 implements bounded gain interpolation across fixed 12-second windows. Musical-section recognition and arbitrary section-specific processing remain proposals.

## D007: Compressor experiment

- Recorded: 2026-09-24.
- Origin: Continuation request following D005 and the mastering-chain review.
- Status: Provisional.
- Implementation: Separate compressor DSP and automatic candidate evaluation in the working tree.
- Supersedes: D002's active chain description and D005's unimplemented proposal.
- Decision: Add linked soft-knee compression after tone and side EQ, before oversampling and final limiting.
- Decision: Compare bypass, full-band detection, and bass-reduced detection without adding user controls.
- Reason: Separate envelope shaping from fast peak control while retaining a complete uncompressed comparison.
- Selection: Require lower mean limiter reduction at similar achieved loudness, plus source-preservation and spatial checks.
- Interaction: Keep selected compressor settings during imaging, then repeat compression benefit checks against the original bypass.
- Budget: Bound the sum of compressor and limiter maximum reductions by the existing peak-reduction budget.
- Alternatives: Always-on compression, automatic makeup gain, hardware emulation, or a full search of every stage combination.
- Tradeoff: Sequential search limits runtime but can miss useful compressor/imaging combinations.
- Limits: Lower limiter workload does not prove less audible damage; fixed timing and source gates require listening validation.
- Evidence: [DSP and policy](../src/compressor.rs), [signal-path tests](../tests/compressor.rs), and [complete-chain tests](../src/pipeline.rs).
- Source interpretation: D005 records the tutorial context; the numerical settings are our engineering choices.
- Verification: Results belong in the dated updates below; measurements and listening judgments remain separate.
- Review condition: Listening evidence contradicts selection, rejects retained transients, or identifies spatial or modulation artifacts.
- Work reference: P2 adaptive limiting and perceived density; technical definition.

### D007 updates

- 2026-09-24: Formatting checks, Clippy with warnings denied, and all 66 release tests passed.
- 2026-09-24: Trials now target the bypass master's achieved loudness. Each attempt records its effective target separately from requested intent.
- 2026-09-24: An earlier full confirmation-mix experiment rejected both compressor trials and retained the previous output exactly.
- 2026-09-24: Successful default-policy compression selection and matched-loudness listening validation remain open. The stage remains provisional.

## D008: Expressive control proposal

- Recorded: 2026-09-24.
- Origin: User questions about plugin color and expressiveness through core controls and spectral analysis.
- Status: Proposed.
- Implementation: Existing intent controls remain unchanged; harmonic coloration is not implemented.
- Proposal: Map a few musical controls to coordinated tone, envelope, nonlinear, and stereo processing behaviors.
- Reason: A small interface can control many internal parameters without requiring processor-specific user settings.
- Distinction: Frequency balance, nonlinear harmonics, and signal-history-dependent response are different processing behaviors.
- Alternatives: Expose every parameter, reproduce named hardware, or use only static spectral matching.
- Evidence: A DAW compressor manual describes circuit families and different responses.
- Limits: That manual does not disclose the vendor's algorithms or establish equivalence with our engine.
- Limits: Spectral measurements alone cannot establish musical intent, sufficient expressiveness, or preferred sound.
- Verification requirement: Multiple-level tone tests, transient tests, aliasing and modulation measurements, and controlled listening across source material.
- Review condition: A control has a repeatable musical meaning and useful range without concealing harmful interactions.
- Work reference: Open decisions.

### D008 updates

- 2026-09-24: The user clarified that FET was one example. D009 defines the broader investigation scope.
- 2026-09-24: D013 adds measured harmonic-color and clipping trials under the existing intent controls. Expressive range and preference remain unvalidated.

## D009: Audio science scope

- Recorded: 2026-09-24.
- Origin: User request to focus on audio science, with no emphasis on a particular circuit family.
- Status: Accepted.
- Implementation: Frequency, temporal, stereo, and peak measurements exist. Nonlinear and modulation measurements remain planned.
- Decision: Define processing through measurable signal behavior and testable listening hypotheses.
- Scope: Frequency and phase response; harmonic and intermodulation distortion; envelope response; gain modulation; stereo interaction; aliasing and reconstructed peaks.
- Reason: A control can change several signal properties. A spectrum alone cannot describe its complete effect.
- Distinction: STFT retains local complex frequency information. Mel power aggregates spectral energy and omits phase.
- Distinction: Neither measurement establishes instrument identity, artistic intent, or listener preference.
- Alternatives: Organize processing around named products, circuit labels, or static spectral matching alone.
- Evaluation: Use multiple input levels, single tones, tone pairs, impulses, bursts, stereo fixtures, and reserved music recordings.
- Evaluation: Separate numerical correctness, processing limits, and randomized matched-loudness listening results.
- Limits: Measurable changes do not prove perceptual improvement. Additional distortion can be intentional; low distortion alone does not define successful mastering.
- Verification: Each experiment must state its expected behavior, counterexample, measurements, and listening evidence.
- Review condition: A measurement fails to predict its intended audible effect or misses a reproducible artifact.
- Work reference: P0 analysis and comparison tests, P2 adaptive limiting and perceived density, and [D008](#d008-expressive-control-proposal).

### D009 updates

- 2026-09-24: The user endorsed the signal-processing and psychoacoustics foundation and requested it as the README introduction.
- 2026-09-24: The README now leads with that foundation, the five evaluation dimensions, and the bounded comparison approach. Runtime behavior remains unchanged.
- 2026-09-24: D013 adds calibrated nonlinear and envelope experiments.

## D010: Metric-led development phase

- Recorded: 2026-09-24.
- Origin: User request to defer A/B listening and continue through audio science and metrics.
- Status: Accepted.
- Decision: Continue with synthetic ground truth, measured processing behavior, and numerical regression tests.
- Decision: Defer user listening comparisons; they do not block the next development steps.
- Reason: The current work establishes DSP and controller behavior before a preference study.
- Alternatives: Require immediate user listening or treat numerical scores as listener preferences.
- Limits: A passing metric supports only the behavior it measures. This decision does not establish perceptual superiority.
- Verification: Record units, source signals, expected behavior, observed results, and measurement limitations for each experiment.
- Review condition: The user requests listening evaluation, or a proposed claim requires perceptual evidence.
- Work reference: Next implementation task.

### D010 updates

- 2026-09-24: The user requested original and mastered confirmation files for listening. Exact full-track copies were provided without loudness matching.
- 2026-09-24: This request does not establish a controlled listening result or replace the current numerical development criteria.
- 2026-09-24: The user reported that the complete master has "more lift." This is an informal delivered-level impression, with the master approximately 2.36 LU louder.
- 2026-09-24: The original listening files remain unchanged during D015 development. The comment does not identify which processing stage caused the impression.
- 2026-09-24: The user also described the result as conservative. Loudness and density tuning remain later work; current implementation keeps its existing thresholds.
- 2026-09-24: A proposed louder render was stopped after the user clarified that this tuning was not needed now. It produced no output files.

## D011: Bounded loudness search

- Recorded: 2026-09-24.
- Origin: Controller inspection during D010 implementation.
- Status: Provisional.
- Implementation: Bounded scalar search and complete-output measurement in the Rust renderer.
- Decision: Replace the initial render plus one correction with at most eight probes measured after final peak trim.
- Decision: Retain the closest measured output; record each probe, bounds, and termination reason.
- Decision: Reserve the measured compressor maximum before calculating the limiter drive bound.
- Reason: Incomplete level matching can confound processor comparisons. The existing combined-reduction check also needs a consistent drive bound.
- Alternatives: Keep one correction, increase drive without bounds, or exhaustively render a dense drive grid.
- Tradeoff: Additional probes increase runtime and temporary memory use. Existing preservation checks remain unchanged.
- Evidence: Search definition, [analytic fixtures](../tests/search.rs), and [complete-chain tests](../src/pipeline.rs).
- Limits: Search termination does not prove global infeasibility, perceptual improvement, or successful compressor selection.
- Limits: The meter remains unchanged; this work does not establish independent loudness or true-peak conformance.
- Review condition: A reproducible level-matching failure, excessive render cost, or an incorrect recorded stop reason.
- Rule context: `engineered-rules.4`; `loudness.search.v1`; plan and render schemas 5.
- Supersedes: D007's previous loudness-search implementation, while retaining its provisional compressor objective and preservation limits.

### D011 updates

- 2026-09-24: Formatting checks, Clippy with warnings denied, and all 73 release tests passed.
- 2026-09-24: A five-second synthetic complete-chain case reached approximately -16.31 LUFS against a -16.30 LUFS target, within 0.05 LU.
- 2026-09-24: That case required more than two probes and passed the unchanged peak and preservation checks.
- 2026-09-24: Existing exact-bypass rejection tests passed. No full-recording render, independent conformance run, or listening test occurred in this step.

## D012: yl.vin Tracks integration

- Recorded: 2026-09-24.
- Origin: User request to add the mastering UI to yl.vin's Tracks UI after an initial engine version.
- Status: Accepted.
- Implementation: Planned; no yl.vin UI or deployment changes occurred in this step.
- Decision: Complete the initial engine milestone, then integrate mastering into the existing Tracks UI.
- Decision: Retain basic musical intent inputs, automatic width, source preservation, and explanations from actual render reports.
- Reason: Mastering belongs in the existing track workflow. Current development remains focused on DSP and numerical evidence.
- Alternatives: Build the UI immediately or introduce a separate mastering application.
- Limits: This decision selects the integration location and sequence, not the execution host, transport, or deployment architecture.
- Limits: UI integration does not establish release readiness or perceptual superiority.
- Verification requirement: Track-to-source association, render status, output association, and displayed decisions must agree with the engine result.
- Review condition: The user changes the integration target or implementation evidence requires a different delivery design.
- Work reference: Decision interface and future customization.

## D013: Basic complete stereo chain

- Recorded: 2026-09-24.
- Origin: User request to continue until a basic complete chain covers the interviewed engineers' workflows.
- Status: Provisional.
- Implementation: Conditional correction, dynamic bands, section gain, domain EQ, harmonic color, clipping, and adaptive limiting are implemented.
- Scope interpretation: Cover transferable stereo-file processing categories; document vinyl, Atmos, stem, editorial, and proprietary boundaries separately.
- Decision: Compare the expanded finishing chain with the retained base chain at similar achieved loudness.
- Decision: Keep source-preservation and spatial checks, and retain exact base audio when finishing fails.
- Decision: Require a triggered correction, requested character change, or at least 0.05 dB lower mean limiter reduction.
- Reason: Add the missing processing categories while preserving explicit objectives and counterexamples.
- Alternatives: Enable every stage unconditionally, emulate named products, or search every stage combination.
- Tradeoff: A bundled sequential trial bounds complexity but can reject useful individual stages together with an unsuccessful stage.
- Limits: Triggered energy dominance does not prove an artistic defect. Small gain changes can still affect intended transitions.
- Limits: Core coverage does not establish that every engineer would approve the workflow or output.
- Evidence: Chain definition, source mapping, and new DSP/controller tests.
- Measurement: The Rust bench calibrates harmonics, selected intermodulation products, modulation sidebands, alias rejection, and envelope recovery.
- Verification: Numerical results belong in the dated updates; listening remains deferred under D010.
- Review condition: A known artifact, false correction, source-preservation failure, or contradictory numerical fixture.
- Rule context: `engineered-rules.5`; `finishing.rules.1`; plan and render schemas 6.
- Updates: D006 now has a bounded implementation, without musical-section recognition or arbitrary per-section plugin editing.

### D013 updates

- 2026-09-24: Formatting, Clippy with warnings denied, and all 95 release tests passed.
- 2026-09-24: The bench produced 75 spectral cases and four envelope traces. Two music renders passed independent numerical checks.
- 2026-09-24: Finishing passed on a 30-second excerpt of a confirmation mix but failed on the complete recording under its different intent settings.
- 2026-09-24: The complete recording retained the base chain, with gentle widening and compressor bypass. Runtime was 303.847 seconds for 232.913 seconds of audio.
- 2026-09-24: Measurements, fingerprints, and limits record both outcomes. No listening preference is established.
- 2026-09-24: D015 replaces the all-or-nothing finishing selection with bounded stage-family alternatives. DSP parameters and preservation bounds remain unchanged.

## D014: Delivery and source-relative album pass

- Recorded: 2026-09-24.
- Origin: Complete-chain request and output/album concepts from a second engineer, a third engineer, and a fourth engineer.
- Status: Provisional.
- Implementation: Source-rate conversion, final dither, decoded WAV verification, ordered album rendering, and source-relative attenuation are implemented.
- Decision: Convert supported stereo sources to the 48 kHz engine; export float32, PCM16, or PCM24 at 48 kHz.
- Decision: Apply deterministic unshaped TPDF dither only at final integer conversion; default output is PCM24.
- Reason: Make quantization and decoded-file verification explicit final processing steps.
- Alternative: Float-only export or unverified integer conversion.
- Decision: Preserve the submitted inter-track integrated-level differences through a common achieved gain and attenuation-only final alignment.
- Reason: Keep an initial EP policy that does not infer interlude roles or force equal song levels.
- Alternative: Normalize every track independently to one target or impose a fixed album spectrum.
- Tradeoff: The most constrained track can lower the complete album's output level; subjective album rebalance remains unsupported.
- Limits: Source-relative levels are a policy, not proof of intended final album balance.
- Limits: Output remains stereo WAV; no DDP, gaps, crossfades, Atmos, vinyl cutting, or codec delivery is implemented.
- Verification: Quantization-error bounds, dither statistics, overwrite refusal, sample-rate tests, and relative-level preservation fixtures.
- Review condition: Decoded-output failure, a source-rate regression, or a supplied album whose intended relative levels differ from its mixes.
- Work reference: Delivery and album definition.

### D014 updates

- 2026-09-24: PCM24 music exports passed decoded checks and independent FFmpeg/SciPy measurements. Source and output fingerprints were verified.
- 2026-09-24: Synthetic album and CLI tests passed, including order, duration, relative levels, output verification, and replacement refusal.
- 2026-09-24: Full-EP musical balance, formal meter conformance, and alternative delivery formats remain outside this verification result.

## D015: Finishing stage-family alternatives

- Recorded: 2026-09-24.
- Origin: Continued implementation request and the finishing rejection on the complete confirmation mix recorded under D013.
- Status: Provisional.
- Implementation: Up to four finishing candidates, source-analysis reuse, and per-candidate decision evidence.
- Decision: Test the eligible combined bundle first; after rejection, test correction, character, and peak-control families in that order.
- Decision: Omit inactive families and duplicate combined candidates. Stop at the first passing candidate.
- Decision: Compare every candidate with the same original base master and unchanged preservation bounds.
- Reason: A failed correction must not automatically discard a separately acceptable harmonic-color or peak-control option.
- Reason: Keeping the combined candidate first preserves its previously accepted outputs.
- Alternatives: Retain all-or-nothing selection, relax preservation thresholds, or exhaustively search stage combinations and orders.
- Tradeoff: Rejected combined processing can require additional complete renders. First-passing selection does not establish the best available sound.
- Limits: Correction still groups cleanup, section gain, and band processing. Peak control still groups clipping and adaptive release.
- Limits: A passing objective does not establish that a source needed correction or that listeners prefer the result.
- Evidence: The full-recording counterexample in the complete-chain measurement record; analytic and controller regression tests.
- Verification: Formatting, Clippy with warnings denied, and all 99 release tests passed.
- Verification: An excessive synthetic correction failed; the character-only alternative passed and exactly matched its independent render.
- Verification: The output for the 30-second excerpt of a confirmation mix exactly matches the previous WAV. The combined candidate passed and alternatives remained unevaluated.
- Rule context: `engineered-rules.6`, `finishing.selection.2`, plan/render schemas 7, and `mastering.finishing-result.2`.
- Report: Record baseline evidence, all eligible proposals, selected/rejected/unevaluated status, and nested compression/imaging selection evidence.
- Supersedes: D013's single bundled finishing comparison, not its DSP definitions or acceptance limits.
- Review condition: A source-preservation regression, missing decision evidence, excessive cost, or reserved-recording results that contradict selection order.
- Work reference: Stage coverage and next implementation task.

### D015 updates

- 2026-09-24: The complete confirmation mix rejected combined and correction-only finishing, then selected the character-only alternative. All selection limits remain unchanged.
- 2026-09-24: The new result measures -10.9656 LUFS, approximately 0.0320 LU from the retained base comparison. Requested character supplies its processing objective.
- 2026-09-24: Three finishing trials required 625.501 seconds. This development run was not an isolated performance benchmark.
- 2026-09-24: Independent complete-output validation passed. FFmpeg reports -11.0 LUFS; the SciPy 16× peak estimate is -1.1958 dBTP.
- 2026-09-24: Selection measurements record the candidates, verification scope, fingerprints, runtime, and informal feedback.

## D016: Confirmation corpus audit

- Recorded: 2026-09-24.
- Origin: User request to test varied tracks from the confirmation corpus and establish cases where optional processing remains active.
- Status: Accepted.
- Decision: Audit the eight distinct original confirmation mixes with the same executable, rules, and intent.
- Decision: Exclude later revisions, the duplicate track-a copy, instrumentals, files from an online mastering service, and human masters from this pass.
- Decision: Reuse the complete track-e result whose executable, source, intent, and delivery match this audit.
- Reason: Source variation must influence automatic decisions. One recording cannot establish useful optional-stage behavior.
- Intent: Pop, punch 0.8, warm character, intensity 0.75, vocals enabled, and a -1 dBTP ceiling; PCM24 output.
- Distinction: This common intent controls the experiment. It does not establish each track's best musical settings.
- Evidence requirement: Separate source ineligibility, insufficient workload, rejected rendered trials, selected processing, and unevaluated alternatives.
- Evidence requirement: Report both positive activation cases and bypass counterexamples. Investigate a stage that never earns selection.
- Limits: The selected intent itself excludes clipping; its absence cannot establish a clipping-stage failure.
- Limits: These are development recordings from one collection, not reserved cross-genre release evidence.
- Alternatives: Tune thresholds per song, count every bypass as success, or force stages on to increase activation counts.
- Verification: Full-track source/output fingerprints, decoded export checks, and independent loudness/peak measurements.
- Review condition: Consistent source-based differences remain absent, checks reject valid fixtures, or optional stages have no demonstrated useful selection case.
- Work reference: The confirmation corpus measurements remain separate from any later rule change. Loudness tuning remains deferred.

### D016 updates

- 2026-09-24: All eight full-track outputs passed decoded checks and independent FFmpeg/SciPy validation under the same intent and executable.
- 2026-09-24: Three tracks selected widening, track-h selected section gain, and track-e selected harmonic color.
- 2026-09-24: No track selected the separate compressor: six failed source eligibility, one lacked limiter workload, and one rejected rendered trials.
- 2026-09-24: The other track-a WAV has different container bytes but identical decoded PCM24. It is not an independent source case.
- 2026-09-24: The audit identifies a positive compressor-selection coverage gap. No eligibility or preservation threshold changed during this pass.

## D017: Premix corpus and native-rate validation

- Recorded: 2026-09-24.
- Origin: User request to test premixes with less post-production processing.
- Status: Accepted.
- Decision: Test the seven supplied MP3 premixes as a separate group, using the same mastering executable and common intent as D016.
- Preparation: Decode 160 kb/s MP3 to native 44.1 kHz float32 WAV with FFmpeg 8.1.2. Apply no normalization or denoising.
- Decision: Preserve decoder overshoots in float samples. Let the Rust engine perform its normal 48 kHz source-rate conversion.
- Reason: Additional source arrangements and processing histories can expose eligibility and controller limitations.
- Limits: MP3 can introduce artifacts and overshoots. These inputs do not establish lossless source quality or measured noise reduction.
- Limits: Premix durations differ from the confirmation files. Differences cannot be attributed solely to mastering or production processing.
- Decision: Extend metrics-only validation to supported native source rates and nearest-frame output duration.
- Verification: Source rate/frame metadata, finite decoded samples, hashes, decoded delivery, native-rate FFmpeg metering, and output SciPy 16× peak estimation.
- Verification: Four Python tests cover native float preservation, integer scaling, invalid inputs, and nearest-frame duration checks.
- Limits: Rust source measurements use its 48 kHz conversion; FFmpeg source measurements use the native rate. Meter agreement is not full SRC conformance.
- Alternatives: Resample test inputs externally or treat lossy premixes as lossless references.
- Report: Metrics-only evidence now uses `mastering.validation.3` and records native and analysis sample rates. Runtime report schema remains 7.
- Review condition: A rate-conversion error, ambiguous source identity, codec-induced false eligibility, or changed supplied source material.

### D017 updates

- 2026-09-24: All seven full premix renders passed decoded delivery and independent native-rate FFmpeg/SciPy validation. All retain EQ and fixed-release limiting.
- 2026-09-24: The track-f, track-h, and track-g premixes select harmonic color. The other four retain the base chain. No extra widening is selected.
- 2026-09-24: No separate compressor is selected: four premixes lack the required limiter workload, and three fail the full-scale-sample eligibility gate.
- 2026-09-24: The three gated inputs contain 1, 6, and 10 full-scale channel samples after conversion, with no flat-top runs. Float-decoded MP3 overshoots require separate interpretation from clipping evidence.
- 2026-09-24: The track-e character trial and the track-d peak-control trial fail solely on measurement coverage. Keep these cases for validity-floor regression tests.
- 2026-09-24: Independent 16× peaks range from -1.2937 to -1.2736 dBTP. Maximum output loudness disagreement with rounded FFmpeg readings is approximately 0.0448 LU.
- 2026-09-24: All original, decoded, output, executable, and validator fingerprints were reverified. The seven reports use the same rules and intent as D016.
- 2026-09-24: Premix measurements record the complete results, codec limits, and remaining selection gaps. Thresholds and loudness targets remain unchanged.

## D018: Compressor source eligibility and opportunity evidence

- Recorded: 2026-09-24.
- Origin: User request to refine the zero selected compressors in D016 and D017.
- Status: Provisional.
- Implementation: Source-indicator separation, explicit workload opportunity, calibrated selection fixtures, and schema-8 reports.
- Decision: Full-scale samples alone no longer veto a compressor trial. Candidate flat-top runs retain the conservative veto.
- Reason: Crossing full scale does not establish clipped waveform shape. Float samples and codec reconstruction can exceed that level.
- Decision: Preserve the current source reduction cap, flat-top detector, DSP settings, and color/clipping eligibility.
- Reason: Separate one eligibility change from loudness, distortion, and finishing-policy changes.
- Decision: Retain the 0.1 dB mean limiter-relief objective and its workload gate.
- Reason: Available mean relief cannot exceed baseline mean limiter attenuation. A smaller workload cannot satisfy this objective.
- Decision: Record the objective, baseline limiter statistics, measured opportunity check, and source indicators separately.
- Reason: Source ineligibility, insufficient workload, and failed rendered benefit are different decisions.
- Evidence: Two six-second noise-burst fixtures select compression under the previous and revised default policy.
- Evidence: Two default-control fixtures skip trials for insufficient workload. Four higher-intensity diagnostic cases reject rendered trials.
- Limits: The positive outputs differ from bypass by approximately 0.174 LU, within the existing 0.2 LU limit.
- Limits: These fixtures establish selection behavior, not exact loudness matching, listener preference, or positive music-selection coverage.
- Limits: The existing flat-top detector is gain-dependent and can flag intentional plateaus. It does not identify distortion or prior processors.
- Alternatives: Force stage activation, reduce the benefit threshold, change compressor timing, or add a new musical objective before calibration.
- Verification: All 104 release tests, four Python validator tests, formatting, and Clippy with warnings denied passed.
- Verification: All eight probe outcomes and final limiter statistics match the frozen previous-policy probe.
- Rule context: `engineered-rules.7`, `compression.rules.2`, `compression.eligibility.v2`, and `compression.opportunity.v1`.
- Report: Plan/render schemas 8; `mastering.compression-result.2`. Current tools consume the new contract directly.
- Supersedes: D007's full-scale-sample veto. Compressor DSP, benefit criteria, and preservation bounds remain unchanged.
- Review condition: A source-preservation regression, unreliable clipping evidence, failed calibrated selection, or measured benefit explained by level mismatch.
- Work reference: Refinement measurements and remaining experiments.

### D018 updates

- 2026-09-24: The track-c, track-d, and track-b premixes pass revised compressor source eligibility with unchanged intent and source reduction caps.
- 2026-09-24: The track-c and track-b premixes lack sufficient limiter workload. The track-d premix renders both proposals and rejects their benefit, loudness, and bass-width results.
- 2026-09-24: All three final WAV files exactly match the previous-policy outputs. New native-rate FFmpeg/SciPy validation passes.
- 2026-09-24: Positive synthetic selection is now covered. Positive music selection and tighter level-controlled benefit evaluation remain open.
- 2026-09-25: D019's tighter comparison rejects both positive synthetic cases. Their earlier selection remains historical controller behavior, not validated benefit.
- 2026-09-25: D023 replaces the source reduction cap after the loudness audit. Flat-top compression eligibility remains separate and unchanged.

## D019: Level-matched compression benefit

- Recorded: 2026-09-25.
- Origin: User request to continue D018's controlled benefit investigation.
- Status: Provisional.
- Implementation: Bounded compression-bypass reference rendering, same-width comparisons, explicit reference evidence, and rejection tests.
- Decision: Compare limiter relief against a separately rendered compression-bypass reference at the candidate's measured final loudness.
- Decision: Require at most 0.02 LU reference mismatch. The reference search targets 0.005 LU with at most 12 evaluations.
- Decision: Keep the candidate's EQ, finishing, limiter mode, width filters, and source-preservation bounds in the reference.
- Reason: Quieter output and different width processing can change limiter workload without establishing a compressor benefit.
- Decision: Preserve the original bypass for fallback, delivered-level comparison, and existing additional-damage checks.
- Decision: Skip reference rendering when earlier preservation checks fail. Reuse the original bypass only at unchanged width within 0.005 LU.
- Decision: Keep the 0.1 dB mean-relief requirement and all existing DSP settings, source gates, main targets, and reduction bounds.
- Limits: The original-workload gate remains a conservative prefilter. It does not prove infeasibility against every possible matched reference.
- Evidence: The previous 0 ms and 20 ms rise fixtures show only 0.0091 and 0.0359 dB mean relief after matching.
- Evidence: Their reference-level errors are approximately 0.00028 and 0.00381 LU. Both fail the unchanged 0.1 dB requirement.
- Correction: The previous two positive cases depended mainly on approximately 0.174 LU quieter output. Positive default-policy audio coverage is reopened.
- Alternatives: Tighten the original comparison tolerance alone, normalize completed outputs without recalculating limiter work, or lower the benefit requirement.
- Tradeoff: Passing preservation checks can require an additional complete reference render. Failed level matching rejects the trial.
- Report: Plan/render schemas 9, compression result 3, imaging result 2, and explicit reference fields on compressor and imaging trials.
- Rule context: `engineered-rules.8`, `compression.rules.3`, and `compression.matched-reference.v1`.
- Limits: Matched workload is a numerical objective, not proof of lower audible distortion or listener preference.
- Review condition: A failed level match accepted as benefit, mismatched width settings, source damage, or a positive case explained by comparison bias.
- Work reference: Matched-reference measurements. Broader envelope objectives require separate calibration.
- Supersedes: D018's near-level benefit comparison and its positive-case interpretation; source-eligibility changes remain active.

### D019 updates

- 2026-09-25: All 108 release tests, four native-rate validator tests, formatting, and Clippy with warnings denied passed.
- 2026-09-25: The eight-case probe selects no compressor. Initial base-chain evidence records the matched-reference rejection of both earlier positive cases.
- 2026-09-25: The track-c, track-d, and track-b full-premix outputs remain byte-identical to D018 and pass independent numerical checks.
- 2026-09-25: The track-d premix fails earlier delivered-level and bass-width checks, so matched references remain unevaluated instead of recording invented benefit values.
- 2026-09-25: Original source, intent, and preservation bounds remain fixed. The next objective work requires new calibrated positive and negative audio cases.
- 2026-09-25: A ten-case envelope audit with six synthetic sources and three intent settings selected no compressor. Two closely matched cases saved at most 0.0383 dB mean limiter work, below the unchanged 0.1 dB rule; one increased limiter work. Other cases lacked opportunity or failed earlier loudness or spatial checks. No production rule changed.

## D020: Defer automatic compression for initial beta

- Recorded: 2026-09-25.
- Origin: Initial integrated-beta finish line and the ten-case matched-level envelope audit.
- Status: Accepted for the initial beta.
- Implementation: `compression.rules.4`, `engineered-rules.9`, plan/render schema 10, compression-result schema 4.
- Decision: Keep separate linked compression bypassed on every beta render. Record eligible proposals as `not-evaluated` with an explicit deferred reason. Preserve the source-eligibility and workload measurements for later research.
- Reason: None of ten level-controlled envelope cases qualified under the unchanged 0.1 dB mean limiter-relief objective. The earlier positive cases were explained mainly by lower delivered loudness.
- Alternatives: Force compression, lower the objective, or add another processing goal. Each requires new controlled evidence before production selection.
- Evidence: Envelope audit, matched comparison, and beta audit.
- Verification: The full release suite, formatting, Clippy, and the independent delivery checks in the beta audit pass. Three representative full-track WAVs remain byte-identical after this reporting change.
- Limits: This does not establish that compression lacks musical benefit. The experimental controller and DSP remain available to controlled tests; no automatic beta render selects them.
- Review condition: Calibrated level-matched positive cases, negative controls, and listening evidence support a revised benefit rule.

## D021: Browser render and report boundary

- Recorded: 2026-09-25.
- Origin: User request for yl.vin Tracks integration after the engine beta audit.
- Status: Provisional.
- Implementation: The WebAssembly build exposes the same Rust engine through `render_pcm16`; the app's browser worker decodes a stored SoundKit stream, renders a 48 kHz stereo float WAV, and writes an immutable job with WAV, full report, summary, and completion marker.
- Decision: Accept mono or stereo decoded PCM16 at supported rates, three seconds to five minutes, explicit intent, and an output peak at or below the requested ceiling. Keep each render beside its source track without replacing the source or earlier renders.
- Decision: The complete report includes the decoded PCM16 SHA-256, input/output frame geometry, intent, processing selection, rejected trials, and render duration. The browser's comparison source uses the same Rust rate conversion as the master and requires equal output frame counts.
- Reason: This is a concrete, inspectable job boundary for a local Tracks beta. The completion marker hides interrupted partial jobs from the UI.
- Evidence: Beta audit and the app's browser integration test (`bitneedle-app/scripts/mastering.browser.mjs`).
- Limits: Browser storage is local to the device. The source is the library's decoded SoundKit stream, which may be lossy when no lossless stream exists. The five-minute bound is an input guard, not a guarantee on every device; measured memory use is documented in the beta audit.
- Review condition: Browser memory failure, output geometry mismatch, unsafe decoded output, or a better bounded execution architecture.

## D022: Absolute source and master visual measurements

- Recorded: 2026-09-25.
- Origin: User request for a DAW-style level waveform, stacked spectral timelines, and an audible original/master TAPE comparison at delivered levels.
- Status: Accepted.
- Implementation: `waveform.rs` computes 384 equal-time bins for both the 48 kHz rate-converted input and selected final output. Plan and render schemas are 11. The browser draws static OLED rows from the saved report, and the vin deck plays the original PCM and saved master WAV on a common frame clock.
- Decision: Each bin records absolute maximum channel sample amplitude and stereo RMS on a shared linear full-scale scale. Report exact sample spans, units, sample rate, and definition. Do not normalize either waveform independently.
- Decision: Display existing half-second six-band Rust stereo measurements for both versions on the same fixed dBFS power scale. Leave source switching, position, lock groove, and CH metering to the deck.
- Reason: Independent per-version scaling hid delivered loudness differences, and browser-side waveform analysis duplicated engine facts. Comparison gain changes also concealed the actual delivered level.
- Alternatives: Normalize both versions for listening, generate relative waveforms, infer loudness from pixels, or measure spectra during browser playback. These do not meet the requested absolute comparison.
- Evidence: `waveform.rs` tests prove a fourfold input amplitude difference stays fourfold in peak/RMS bins and verify stereo RMS; the app's browser integration test (`bitneedle-app/scripts/mastering.browser.mjs`) measures distinct audio output RMS after TAPE switching, equal frame geometry, and visible aligned report rows.
- Limits: Sample peak and RMS envelopes do not replace integrated LUFS or reconstructed true-peak metering. A source above the requested loudness target can produce a quieter master. Matching playback level remains useful for controlled listening research, but the product comparison plays raw delivery levels.
- Review condition: A frame alignment error, misleading visual amplitude scale, report measurement mismatch, or deck output diverging from selected PCM.
- 2026-09-27 replacement update: [D086](#d086-show-selected-mastering-controls-and-measured-limiter-activity) replaces the sampled frequency-change report view. The absolute waveform measurement and its limits remain relevant.

## D023: Source peak cap and preservation search

- Recorded: 2026-09-25.
- Origin: The user heard quieter masters than the confirmation mixes and asked for a loudness fix against the online mastering service and the reference engineer's masters.
- Status: Provisional.
- Implementation: `engineered-rules.10`; `source.clipping.v1` reports indicators without the former 1.5 dB cap. The limiter retains the original intensity-and-punch reduction budget, true-peak ceiling, and measured preservation thresholds. The controller tries up to six coarse drives, then three midpoint refinements after a pass. It records the selected passing candidate even if the last probe fails. The browser starts at 80% intensity and 50% punch.
- Reason: Six of the eight confirmation mixes triggered the source cap from 18–140 full-scale channel samples or 1–16 candidate flat-top runs. A 30-second track-f trial showed that a 1.5 dB backoff skipped a passing drive near the source level. Those indicators alone do not establish that every additional dB of peak control will distort the track.
- Alternatives: Keep the 1.5 dB cap, raise attack-loss tolerances, or relax the −1 dBTP ceiling. The cap repeatedly made output quieter. The latter changes need separate listening and delivery decisions, so this change preserves both limits.
- Evidence: The confirmation corpus measurements document the earlier six-track result. The loudness-repair run records the new source-relative and independent peak measurements. All rejected trials remain visible in the render report. The reference master of track-e is an offline comparison, not an engine target.
- Limits: The search is bounded and can still fall short of its requested LUFS. True-peak compliance and measured attack preservation do not prove absence of audible distortion. The human reference reaches −7.2 LUFS at approximately −0.1 dBTP, outside this −1 dBTP test policy.
- Review condition: Full-track validation, independent measurements, and level-matched listening reveal a regression or support a change to peak, attack, or tonal policy.

### D023 update

- 2026-09-25: Second review found that the accepted track-a and track-f passes stopped after one loudness probe at the preset 3.24 dB peak budget. track-e stopped at its chosen −11.2 LUFS target. These results did not establish the loudest acceptable sound. D024 replaces the loudness ranges and preservation policy; the removal of the source-indicator cap and the refinement algorithm remain.

## D024: Treat delivered loudness as a primary mastering objective

- Recorded: 2026-09-25.
- Origin: The user judged the repaired masters still much too quiet and clarified that the online mastering service provides an expected loudness comparison while its sound is not a quality reference.
- Status: Provisional.
- Implementation: `engineered-rules.11`, `intent.density.v2`, and `preserve.punch.v2`. Exact mappings are in [RULES.md](RULES.md). The browser's neutral / intensity 0.8 / punch 0.5 intent now aims at −10 LUFS. Acoustic target endpoints remain −20 to −13 LUFS.
- Decision: Give the loudness search a larger peak-reduction budget, allow more intentional dynamic change at moderate punch, and add an independent mean-limiter-reduction limit. At maximum punch, retain the previous attack, crest, and window allowances. Permit ten coarse backoff candidates so the expanded drive budget can still reach a minimally limited candidate before considering gain-only output.
- Reason: A maximum reduction from one brief peak is not the same measurement as persistent compression. The previous 3.24 dB budget stopped accepted outputs before testing competitive levels. Its 1.5 dB attack allowance was an uncalibrated product restriction, not a demonstrated threshold for audible damage.
- Alternatives: Increase output gain alone, raise the delivery ceiling, force every track to the reference master's −7.2 LUFS, or remove preservation checks. This revision keeps the −1 dBTP browser ceiling and bounded backoff. It can still fall below its aim when measured changes exceed its provisional limits.
- Evidence: Earlier full-song measurements and the new loudness-objective run. The evaluation tools (`examples/loudness_probe.rs` and) expose level/workload curves and gain-invariant differences separately from product selection. Aligned measurements of the online mastering service describe its changes, not their desirability. Comparisons with the reference masters are also measurements, not proof that our processing achieves equivalent sound.
- Tutorial context: In a tutorial video on mastering a hip-hop single (06:21–06:49), a third engineer describes backing drive down in half-decibel steps when the result smears or overdrives. This supports iterative auditioning; it supplies none of our numerical thresholds.
- Limits: Mean gain reduction, crest, onset and window metrics do not fully detect harshness, pumping, altered harmonics or vocal damage. Peak compliance is a delivery check. No listening preference or equal-quality claim is established by this change. This collection is development material, not held-out validation.
- Verification: The run document records full-song, native/WASM, regression, independent-meter and source-preservation results separately from the still-open listening judgment.
- Review condition: Matched-level listening exposes a worse sound, excessive compression, loss of musical contrast, or a need for better limiting/correction rather than more permissive thresholds.
- Supersedes: D023's unchanged target ranges and preservation budgets.

## D025: Separate intentional tone changes from spatial damage

- Recorded: 2026-09-25.
- Origin: Second review found every rules-11 full-song finishing bundle rejected. The intended high-band reduction of track-e was rejected solely because its 0.533 dB center-energy reduction exceeded the 0.35 dB imaging limit.
- Status: Provisional.
- Implementation: `engineered-rules.12`, retained by revision 13; `finishing.selection.3` and `mastering.finishing-result.3`.
- Decision: For finishing comparisons, measure center and mono shares within each band and separately constrain the matched total band-energy change to 1.5 dB. Select measurement coverage from the baseline, preserving raw candidate powers when a small cut crosses its independently computed activity floor. Keep the width selector's absolute-energy checks.
- Reason: The old comparison confused an intended linked EQ change with loss of center image. It could veto a 0.75 dB corrective cut simply for working.
- Alternatives: Remove imaging checks, increase all spatial allowances, or force finishing selection. The new path retains actual correlation, balance, low-bass width, spatial variation, workload, loudness, crest and attack rejection.
- Evidence: Regression tests accept a linked 0.75 dB treble cut under tonal comparison while the width comparison still rejects its absolute center change. A 6 dB EQ cut and actual mid/side damage remain rejected. The full rules-12 test of study track 1 still rejects generic finishing for other measured failures and retains the base result; this is not a force-on fix.
- Limits: Band-normalized comparisons can still respond to changes in within-band spectral composition. The 1.5 dB bound is an engineering hypothesis, not a perceptual guarantee.
- Verification: Rules 12 passed 115 release tests and clippy. The stem-analysis run records revision-13 verification and real-audio outcomes.
- Review condition: Matched listening or controlled spatial counterexamples demonstrate damage that the revised comparison misses, or useful bounded correction is still incorrectly vetoed.
- Supersedes: D016/D017 finishing reuse of absolute width-comparison center/mono checks only.

## D026: Use existing stem estimates to inform original-mix decisions

- Recorded: 2026-09-25.
- Origin: The user explicitly requested use of our stemsplits/HTDemucs implementation to locate vocals and frequency activity, and selected study track 1 without a reference master.
- Status: Provisional.
- Implementation: `engineered-rules.13`, plan/render schema 12, `stem-evidence.1`, and `finishing.stem-vocal.1`. Native `--stems` and WASM/browser use the same Rust analysis and selection. Separation remains in stemsplits, outside the mastering render.
- Decision: Measure aligned, unnormalized drums/bass/other/vocals estimates. Check their sum against the original mix, record exact PCM fingerprints and activity/spectral evidence, and optionally propose a single bounded vocal-associated treble trial on the original stereo mix. Preserve the base and generic alternatives. Existing completed library STEMS are used when vocals are enabled; MAKE MASTER does not implicitly launch a cloud split.
- Reason: Whole-mix energy at a fixed 7.5 kHz cannot establish whether a peak belongs to vocals or drums. The supplied estimates can locate likely vocal-associated activity and constrain the choice of band, although they cannot establish isolation or a defect.
- Bounds: At least three seconds of estimated vocal activity; sum residual no greater than −18 dB relative to source, cosine at least 0.98, and duration within one converted 48 kHz frame. The located 5–11 kHz region needs at least 50% estimated vocal power share and intermittent relative excess; the original mix must corroborate the existing dynamic-cut trigger. Q 1.4, maximum 0.75 dB reduction, no automatic body/presence boost. Exact rules are in [RULES.md](RULES.md).
- Alternatives: New separator/runtime in the engine, process and remix separated audio, blindly boost the strongest vocal band, or treat all treble peaks as sibilance. These either duplicate stemsplits, expose the output to separation artifacts, or infer a defect without enough evidence.
- Evidence: The existing local HTDemucs split of study track 1, independent SciPy analysis, Rust plan and original-mix renders are recorded in the run. The estimated vocal treble candidate is approximately 9.05 kHz. Source-sum similarity is an integrity check, not evidence that every stem label is correct.
- Limits: Bleed and model artifacts can mislead analysis. The stereo-linked cut also affects other instruments in that band. This is a conditional dynamic-band trial, not a general vocal-remixing system or a complete multiband compressor. Body/presence energy does not by itself justify brightening. Listening validation remains necessary.
- Verification: Tests cover changed vocal frequencies, source preservation, stale evidence, shifted/normalized estimates, invalid geometry and nonfinite samples. Native, WASM and browser checks are recorded separately from listening conclusions.
- Review condition: Listening rejects the candidate, bleed repeatedly mislocates it, or representative music supports a different threshold, context window or processing objective.
- 2026-09-25 implementation update: Browser stem imports now preserve the received PCM in a FLAC copy, because re-encoding the exact additive regression fixture through Opus caused source-similarity failure. This does not imply lossless separation or restore losses in upstream transport.

## D027: Use instrument evidence for recovery and stronger band control

- Recorded: 2026-09-25.
- Origin: The user requires loudness close to the matching reference master and explicitly requests bolder stem-informed decisions.
- Status: Provisional.
- Implementation: `engineered-rules.14`, plan/render schema 13, `stem-evidence.2`, `finishing.stem-bands.2`, `finishing.selection.4`.
- Decision: Browser-default neutral targets −8 LUFS. Replace sparse whole-mix onset spacing as limiter release evidence with a 50 ms starting point; aligned drum decays and dominant bass period refine it within 35–80 ms. Test stronger instrument-located bass and vocal control, preserving combined/isolated/base alternatives and all existing comparison bounds. Let a successful finishing stage recover available loudness after its matched comparison.
- Reason: On track-e, the full-mix analyzer detects only 12 onsets, with a 20.3-second median gap, producing the old maximum 140 ms release. On the same 30-second excerpt at −8 LUFS, 35 ms release reduced average limiter reduction from 4.74 to 1.49 dB and occupancy above 1 dB from approximately 100% to 67.5%, passing the existing default limits. The old −10 LUFS target itself was substantially below the reference master's −7.2.
- Bounds: Exact timing, spectral eligibility, dynamics and unchanged spatial/source limits are in [RULES.md](RULES.md). Instrumental estimates remain usable even when no vocal proposal exists. No stem waveform enters the rendered output.
- Alternatives: Increase every damage allowance, raise the delivery ceiling to the reference master's, or force correction on. These are not required by the measured release experiment. Existing full-chain limits remain; failure to approach the reference must remain visible.
- Limits: Shorter recovery can increase low-frequency modulation/harmonics. Two bass cycles and bounded band cuts do not prove transparency. Estimated stems can misidentify instruments. −8 LUFS is this provisional product objective, not a universal mastering standard. A louder recovered result passes source constraints but has not been proven perceptually equivalent to the quieter matched trial.
- Verification: See the reference loudness run for current objective tests and listening status.
- Review condition: Matched listening exposes distortion/pumping, another exact-reference pair fails, or stronger control produces no useful headroom/tone benefit.
- Supersedes: D024 target endpoints, D026 single 0.75 dB vocal trial and vocal-only eligibility. Historical measurements remain unchanged.

## D028: Reopen compressor trials against the new loudness baseline

- Recorded: 2026-09-25.
- Origin: The user explicitly asked to redo the trials after reviewing the chain and the blanket compressor bypass.
- Status: Provisional.
- Implementation: `engineered-rules.15`, plan/render schema 14, `compression.rules.5`, `compression.eligibility.v3`.
- Decision: Remove the beta-wide compressor disable. Every eligible render compares fresh full-band and bass-reduced-detector proposals with its current bypass. Preserve all existing matched-level benefit, source dynamics, stereo, peak and workload checks. Report actual applied compressor settings and measured reduction in the web UI.
- Source evidence: Short flat-topped runs remain recorded and warned about, but do not categorically forbid a comparison. The study track 1 source has five candidate runs and zero full-scale samples; that does not establish that no bounded compressor can help it. A comparison can reject harmful output directly.
- Reason: Previous compressor findings used different targets and limiter recovery. A permanent bypass prevented new material and the revised chain from supplying new evidence.
- Alternatives: Force compression on every master, loosen its benefit guards, or retain global bypass. None addresses the request for fresh evidence as directly as existing controlled trials.
- Limits: Mean-limiter relief is one measurable objective, not a complete musical judgment. Fixed proposals remain limited in ratio, attack and release; rejection does not prove every possible compressor setting useless. Extra renders increase runtime. Source eligibility and insufficient limiter workload can still skip unsuitable trials with explicit reasons.
- Verification: The reference and chain retrial run, including separately reported listening status.
- Supersedes: D020 global beta deferral and D018 flat-top proposal veto only. Previous benchmark results remain historical evidence.

## D029: Two finish choices, with no continuous amount control

- Recorded: 2026-09-25.
- Origin: The user wants an opinionated engine with fewer vague controls and rejects an amount slider unless a reliable master can be regenerated quickly from an existing basis.
- Status: Superseded as product direction by D034; the described beta UI remains implemented until replacement.
- Implementation: The app's web preset and UI modules (`mastering-presets.mjs` and `mastering-ui.mjs`); no DSP or native intent contract change.
- Decision: Expose Full master (default) or Minimal, and Natural (default), Warm or Bright tone. Remove genre, punch, intensity and vocal-checkbox controls. Full retains the tested neutral / intensity 0.8 / punch 0.5 intent. Minimal uses neutral / intensity 0.5 / punch 1, requesting −11 LUFS with the existing strict dynamics limits. Both retain vocal/center protection and −1 dBTP delivery. Natural maps to clean character.
- Reason: The user should choose the desired finish and tone while the engine owns processor selection. Full native renders still take minutes with existing stems. Analysis reuse alone does not remove the candidate renders and output measurements needed to establish a changed master.
- Alternatives: An amount slider, more intermediate modes, or a simulated gain-only preview. A responsive control would need measured preview latency and verified agreement with final processing. Current controls initiate a complete render explicitly; completed versions can be loaded without re-mastering.
- Evidence and limits: Full-song Full-master results are in the reference loudness run. Minimal's mapping is a provisional product choice, not a universal target or a guarantee of greater perceived quality. Source-dependent constraints can lower either output below its requested loudness.
- Verification: The app's web verification record records real browser preset renders and independent real-music WASM checks.
- Review condition: Listening supports different finish semantics, or a cached preview pipeline demonstrates fast interaction with consistent final output.

## D030: Retain stronger compressor experiments for human review

- Recorded: 2026-09-25.
- Origin: The user requested different track-e masters for review, explicitly rejected testing only gentle compression, and required master-type filename prefixes for a commercial DAW.
- Status: Provisional experiment; no production selection-policy change.
- Implementation: The listening renderer (`examples/listening_render.rs`), the review package builder and local listening page. Exact saved-basis replay is checked against the approved master. Full-band and bass-filtered gentle replays use their saved trial settings; additional 2:1 and 3:1 profiles use lower thresholds, contrasting attack/release and 3–4 dB maximum compression.
- Decision: Preserve rejected and deliberately firmer outputs for matched listening, retaining true-peak delivery checks and recording source-constraint violations. Keep the original mix, approved master and matching reference master available. Cut listening passages after full rendering. Provide anonymous switching with optional reveal, saved feedback and named full WAVs.
- Reason: Twenty automatic trials reused two gentle settings across different finishing chains. They did not establish that broader compression choices lack musical benefit or that every automatic rejection threshold is perceptually justified.
- Alternatives: Relax production guards immediately, force a compressor on all masters, or continue testing only 1.3:1. Listening examples permit a review before changing the automatic contract.
- Evidence: The track-e listening review records settings, exact replay, whole-song results, independent peaks, matching accuracy and browser verification.
- Limits: Stronger examples can exceed automatic preservation budgets and are not approved masters. Gentle historical trials have unchanged width while firm/current comparisons share the selected side EQ. Labels describe intended treatments, not proven audible results. The informal anonymous page is not a formal ABX study.
- Review condition: User preferences and audible defects support a different proposal family, a measured acceptance rule, or retaining bypass.

## D031: Use the reference master as the provisional track-e calibration reference

- Recorded: 2026-09-25.
- Origin: The user hears the compressor variants as effectively identical, hears greater vocal presence in the reference master with a possible metallic edge, and explicitly requests treating the reference master as canonical while generalising its approach across tracks.
- Status: Accepted evaluation direction; processing-policy changes remain proposed.
- Implementation: Analysis only in an offline script; no production DSP or automatic-selector change.
- Decision: Treat the reference master as the canonical track-e comparison for now. Measure and test tonal balance, vocal presentation, stereo image, phase and peak control separately. Use source and validated stem evidence to place trials on other recordings; do not install the measured response of track-e as a universal target curve.
- Reason: Matched measurements reveal a broad low-mid dip, presence lift and approximately 4 dB side/mid change that our compressor-only variants do not explore. Current imaging policy excludes comparable changes by construction. The user confirms the vocal benefit, which becomes explicit listening evidence.
- Alternatives: Keep adjusting broadband compression only, copy the reference curve to every mix, or treat the possible metallic quality as grounds to discard the reference master. These omit the user's requested calibration and generalisation objectives.
- Evidence: The reference study, including held-out static-transfer prediction, phase/peak ablations, measurement calibration and listening observations.
- Limits: The reference engineer's intent and exact chain are unknown. High coherence and small RMS residual do not rule out important peak limiting. One reference cannot validate a general mastering policy. Metallic vocal character has not been causally isolated. Stem energy does not itself establish a tonal defect.
- Review condition: Separate tone/width/peak-control auditions, then distinct source types, support or contradict a proposed decision family. Preserve source, centre, mono and delivery evidence while testing bolder processing.
- Work reference: task list section "Canonical-reference calibration and source-dependent decisions".

### Update 2026-09-25: track-h preview as a second comparison

- The user identifies the release of track-h as another master by the reference engineer and may obtain the original later. Accept the available lossless 29.952-second preview for provisional local analysis; full-song validation remains pending.
- The excerpt analysis script strongly pairs it with the later confirmation mix of track-h, not the earlier confirmation mix of track-h. Separate-region transfer correlations exceed 0.999 for both mid and side; original delivery provenance is still unavailable.
- The reference master is approximately 8.4 LU louder on this passage. At matched excerpt loudness it reduces deep bass, adds low-mid body and high-frequency air, and raises side/mid energy. This contrasts with the low-mid reduction and presence lift of track-e, reinforcing source-dependent trials rather than a universal copied curve.
- Evidence and limitations: The track-h reference preview study. These measurements identify differences, not the reference engineer's precise chain or intent. No production DSP or selector changes. Review the full original's identity, corresponding PCM and section behavior when available.

## D032: Derive treatments from deterministic spectral and auditory diagnostics

- Recorded: 2026-09-25.
- Origin: The user accepts the online mastering service as useful secondary evidence despite limited access to professional masters, then explicitly requires a deterministic spectral/psychoacoustic approach before broader trials.
- Status: Accepted evaluation direction; automatic treatment/acceptance policy remains proposed.
- Implementation: An offline auditory analysis script and a source-only trial proposal script, with known-signal checks in a test script. No production DSP or selector change.
- Decision: Measure source-conditioned auditory-band overlap over time, spatial distribution and headroom evidence first. A proposed treatment must identify the condition, expected effect, collateral costs and a falsifiable comparison. References calibrate decisions; they are not required runtime inputs or universal target spectra.
- Decision: Use fixed ERB/roex diagnostics with explicit limits. Do not present overlapping energy as a masked threshold, mid as isolated vocals, or reference distance as sound quality. Keep delivered loudness and peak measurements separate.
- Decision: Retain the online mastering service for verified-source comparisons of level, tone, width and dynamics. The user's rejection of its sound prevents promoting it to a preferred master by default. Verify mix revision before comparing.
- 2026-09-26 policy replacement: [D063](#d063-reassess-assumptions-across-the-confirmation-mixes-and-the-premixes)
  restricts new evidence from the online mastering service to integrated loudness. Historical measurements
  remain recorded, but tone, width and dynamics are no longer reference objectives
  or support for mastering decisions under the user's explicit clarification.
- Reason: The previous selector emphasizes small passing changes; that is not a demonstrated musical objective. Initial stem analysis shows materially different spatial opportunities in track-e and study track 1. Shared EQ cannot isolate coincident sources at one frequency, while measured M/S differences can support limited differential treatment.
- Alternatives: Broader arbitrary presets, copying the reference curve, requiring more professional references before progress, or inventing a single perceptual quality score. These do not address the requested deterministic reasoning.
- Evidence: Auditory analysis, including source validation, pairing with the online mastering service, known-signal calibration and specific limits.
- Implemented experimental proposal: Require meaningful vocal excitation and eligible duration, median accompaniment dominance, and a positive predicted local M/S ratio response across adjacent filters. Record exact provisional thresholds, response and collateral checks. The measured +3 dB probe is not an export setting. Centre/side/reversed/equal-geometry fixtures verify different and unchanged outcomes.
- Counterexample: Fresh track-h stems yield no spatial-overlap opportunity under this rule, although its reference master widens substantially. Do not turn this narrow diagnostic into a blanket mastering veto. Tonal balance, stereo presentation and peak control require separate explicit objectives.
- Review condition: Check each hypothesis on complete rendered audio and aligned listening passages. Retain neutral and counterexample recordings. Replace any criterion that rewards an irrelevant difference or contradicts controlled listening.

## D033: Render independent complete-chain treatments before selecting a mastering policy

- Recorded: 2026-09-25.
- Origin: The user requests going as far as possible before listening, following deterministic source/auditory analysis and provisional calibration against the reference master.
- Status: Accepted experimental workflow; aesthetic preference and broader production policy remain unvalidated.
- Implementation: `treatment::Tone` and insertion-point dynamic calibration reuse existing Rust DSP. Schema-2 `listening_render`, treatment job/batch/audit/package tools, and manifest-driven OLED listening pages are implemented. The shipping automatic selector is unchanged.
- Decision: Render every explicit unchanged, forward, inverse and combined candidate independently; retain all costs and delivery evidence. Separate matched-level tone/space comparisons from stronger delivery branches. Use reference fits only on the paired track and within verified reference scope.
- Reason: Previous compressor-only alternatives missed the measured tonal/spatial differences. Narrow preservation bounds and first-passing selection cannot establish a musical objective. Complete-chain measurements can test whether a treatment actually survives limiting.
- Alternatives: Copy one reference EQ to every track; promote numerical similarity to quality; add a second compressor regardless of measured benefit; hide experiments that fail old source budgets; extrapolate a preview's loudness to the whole track.
- Evidence: Treatment study: 27 complete renders, exact control replays, independent loudness/peak checks, strong tonal/spatial movement, modest source-only ratio improvement, weak bass-headroom benefit, and a less-wide track-h follow-up prompted by full-song mono evidence.
- Bounds: Experimental linear filters are at most eight per domain, ±6 dB per filter and ±9 dB total domain response. Measured dynamic cuts are at most 3 dB. These are not new production acceptance thresholds.
- Interface: Schema-1 listening jobs are replaced outright. Schema-2 review manifests specify track, passages, duration, initial selection and references. Preserve type-first full filenames, local feedback, matched/delivered playback, and a separate opening-only track-h reference page.
- Verification: 125 Rust release tests; fmt/clippy; explicit frequency/domain and detector calibration tests; Python presence-selection regression; complete independent export checks; actual browser PCM/gain and interaction verification. Initial track-e and study track 1 feedback is recorded below; broader listening validation remains pending.
- Limits: Reference distance and constant-gain residual are not perceptual scores. Linear stem analysis is not isolated-vocal evidence after nonlinear mastering. Source-budget violations remain visible. A static half-width trial is not an automatic governor. No professional full track-h reference is available.
- Review condition: Listening preferences and defects must support a source-dependent objective and acceptance rule across other tracks, with unchanged and contrary cases preserved. Distinguish tonal colour from a requirement to add nonlinear distortion.
- Work reference: task list section "Canonical-reference calibration and source-dependent decisions".

### Listening update 2026-09-25

- The user tentatively prefers version E of track-e (Dense Full), then A (Full). Track identity is confirmed; playback mode and passages are not.
- These share tone/space settings and differ in drive and delivery ceiling. Retain the preference without attributing it to density independently of loudness or ranking unmentioned versions.
- Evidence: treatment-study listening update. This is initial informal evidence, not a general production approval.

### Further listening update 2026-09-25: study track 1

- The user loves both A (Full Vocal Focus, −10.008 LUFS) and C (Dense Vocal Focus, −9.005 LUFS), with no ordering given, and rejects D (the online mastering service) as shallow. Playback mode and passages remain unconfirmed.
- Both preferred versions share the stem-informed broad mid-channel body lift and omit the extra bass stage. This is track-specific positive evidence; level and EQ contributions are not yet isolated.
- Matched spectral measurements provide a plausible body/presence hypothesis for the reported shallowness, not proof of a compression defect. See the study track 1 feedback. Keep the online mastering service as negative comparative evidence.

## D034: Generate masters before asking for preference

- Recorded: 2026-09-25.
- Origin: After hearing distinct interpretations on track-e, study track 1 and track-h, the user says producing these versions is better than asking for **any upfront direction**.
- Status: Accepted product direction; implementation pending. This is not a claim that the current beta already produces a source-dependent set automatically.
- Decision: Start with one Make masters action. Analyze the source and available validated estimates; generate the defensible, distinct full masters supported by the material, with no fixed count or three-version cap; let the user listen and choose one or more. No initial genre, tone, finish, punch, amount, reference or vocal-description question is required.
- Decision: Start comparison at matched loudness and offer explicit delivered-level playback. Keep the original accessible and retain playback position when switching. Show concise result descriptions after processing, with technical evidence in an optional view. Refinement may follow listening but is not mandatory.
- Decision: Keep unchanged/essential processing eligible. Return the useful interpretations the recording supports; more than three is explicitly welcome. Do not discard justified alternatives to meet a quota or force additional processing to fill slots. Suppress near-duplicates and demonstrated technical failures, not personal taste differences.
- Reason: The user can judge actual sound more readily than predict abstract controls. Initial feedback favours different treatments across tracks. The engine should own the proposal work while supporting taste instead of pretending there is one objective aesthetic winner.
- Alternatives: Require users to specify genre/brightness/finish first; expose the entire research grid; return only the first passing render; give every track fixed Warm/Bright/Wide presets; copy the calibration references' curves to unrelated music. These do not meet the new direction.
- Engine work: Source-conditioned proposals without required paired references; shared source analysis; complete-chain renders and independent output checks; a material-dependent set of distinct justified results, excluding near-duplicates and deliberately adverse research controls. Preserve per-version source/renderer/render IDs, measurements, costs and completion state.
- Interface work: Replace the existing preset contract outright when set rendering is integrated. Do not just relabel repeated existing preset runs as the new engine. Use stable version IDs and consistent letter mappings across a track's full and excerpt comparisons.
- Evidence: Treatment study and the user's explicit product preference. track-e tentatively E then A; study track 1 A and C loved; track-h C loved with B close, A possibly harsh and F interesting. The page-specific identity of track-h C is pending clarification. The product decision does not depend on that unresolved mapping.
- Limits: Playback mode is unconfirmed for these conversational observations. They do not isolate every processor's contribution, prove audibility thresholds or establish a universal mastering recipe. Existing full-track reference fits remain calibration experiments, not the finished reference-free proposer.
- Verification: Design recorded in the app contract; implementation and end-to-end verification remain planned. Existing single-master DSP behavior is unchanged.
- Review condition: The set gives users useful alternatives across contrasting sources without redundant options, demonstrated delivery/processing faults, misleading labels or unacceptable render latency. Personal dislike of an intense treatment is not itself a processing fault. Calibrate selection/diversity rules against listening and counterexamples.
- Work reference: task list section "Generate masters before asking for preference".
- Supersedes: D029's upfront Full/Minimal and Natural/Warm/Bright product choices, and D012's requirement for basic musical-intent inputs. The no-continuous-slider constraint and engine-owned width remain.

### User clarification 2026-09-25: preference is not rejection

- The user explicitly says version A of track-h being too far for their taste does not mean reject it. The intended three-version set can include an A-like treatment.
- Retain distinct viable interpretations, including bold ones. Separate listener preference from demonstrated technical failure; no personal-preference veto is added to proposal generation or set selection.

### User clarification 2026-09-25: the material determines the set

- The user explicitly welcomes more than three versions and likes the descriptions. Return the worthwhile versions the material leads to, with no fixed count or three-version cap. This supersedes the earlier three-choice starting hypothesis.
- Retain informative descriptions of completed treatments and effects; they are not upfront direction questions or a generic preset menu.
- The user identifies this as a potential USP. The positioning is accepted; the assertion that every competing service requires upfront guidance is not verified by a market audit.

### Implementation update 2026-09-25

- [D035](#d035-first-native-source-reused-set-path) adds a native set path for existing source-triggered finishing families. D034's broader treatment, delivery and interface work remains pending.

## D035: First native source-reused set path

2026-09-25: Superseded by [D039](#d039-complete-a-native-set-job-using-the-af-treatment-renderer).
The following records the earlier prototype; its set implementation has been replaced.

- Recorded: 2026-09-25.
- Status: Provisional implementation; listening validation pending.
- Decision: Measure one exact 48 kHz stereo source into a memory-only `SourceAnalysis` object, verify its PCM fingerprint before reuse, and rebuild intent-dependent plans from those facts. Start the set with an essential complete-chain master. Independently render all existing source-triggered finishing proposals and retain those passing the current matched full-chain checks plus a provisional −30 dB gain-matched residual screen against every retained version.
- Reason: This creates a genuine material-dependent native set path and preserves per-version render evidence without copying the three study tracks' EQ curves or imposing a fixed count. Source analysis is the first safely reusable branch of the render graph.
- Alternatives: Repeat full analysis for each proposal, expose the research grid unchanged, use genre presets as versions, or persist a cache format before validating the in-memory boundary. These either waste work or claim more proposal quality than the evidence supports.
- Evidence: Existing `finishing.rules.1`, `finishing.stem-bands.2` and `finishing.selection.4` measurements and complete-chain checks. The −30 dB screen is an engineering hypothesis, not a demonstrated audibility threshold.
- Implementation: `pipeline::analyze_source`, `plan_from_source`, `master_from_source` and `master_set`; source fingerprint mismatch fails. Each retained version carries a rule, measured description, audio and full report. All tested branches remain in the trial report, including technical and duplicate rejection evidence.
- Validation state: Reused and fresh plans are compared across intents in regression tests; set shape and source binding are tested. Full-track listening, timing, memory, native delivery, WASM and browser integration are pending.
- Review condition: Listening shows a suppressed useful alternative, a retained near-duplicate or an unhelpful finishing interpretation; representative full tracks show excessive analysis cloning, runtime or memory; or current acceptance checks reject musically valid stronger treatments. Revisit the residual threshold and expand proposal families from that evidence.
- Replaces: No earlier accepted rule. Extends [D034](#d034-generate-masters-before-asking-for-preference).

### Delivery update 2026-09-25

- [D036](#d036-native-set-delivery-boundary) adds the native CLI delivery and progress format. The in-memory set render and provisional acceptance policy above remain unchanged.

## D036: Native set delivery boundary

2026-09-25: Superseded by [D039](#d039-complete-a-native-set-job-using-the-af-treatment-renderer).
The following records the earlier prototype; the v2 native contract replaces it.

- Recorded: 2026-09-25.
- Status: Provisional implementation; full-track resource and interruption testing pending.
- Decision: `master set INPUT.wav NEW_OUTPUT_DIRECTORY` accepts only an export format and optional already-aligned stem estimates. It creates a new directory, writes each retained version as a type-first WAV and immutable JSON report, decodes and verifies each WAV, then writes a final `set.json` manifest. An append-only `progress.jsonl` records start, render completion, each verified delivery and final completion or a caught failure. Refuse an existing directory.
- Reason: A set needs stable result identities, separate measurements and an inspectable completion boundary. A new directory and create-new file writes preserve every source and previous output. Progress events identify completed versions if delivery stops partway through.
- Alternatives: Reuse the single-master output contract, overwrite a manifest after each branch, or expose initial sound-direction flags in the set command. These conflict with immutable version evidence or the accepted one-action workflow.
- Evidence: The first native set path [D035](#d035-first-native-source-reused-set-path), existing decoded delivery verification, and CLI regression tests for source preservation, complete reports, progress and overwrite refusal.
- Validation state: Targeted CLI tests and the required Rust format, Clippy and release test suite pass. Cancellation during the in-memory render, full-track peak memory, WASM and browser integration remain pending.
- Review condition: Full-song tests expose excessive memory/latency, interruption leaves ambiguous state, names fail downstream import, or listening shows that current provisional version selection is too narrow.
- Replaces: No prior set-delivery contract. Extends [D034](#d034-generate-masters-before-asking-for-preference) and [D035](#d035-first-native-source-reused-set-path).

### Measurement update 2026-09-25

- A 30-second study track 1 excerpt run produced one verified essential master in 77.47 seconds with 213,155,840 bytes peak resident memory. Three completed finishing branches failed current comparison checks and the provisional duplicate screen. This is an execution and guardrail observation, not a listening verdict or a full-track performance estimate.

## D037: Reuse identical treatment render work

- Recorded: 2026-09-25.
- Origin: The user clarifies that the immediate engine priority is faster rendering of the already liked listening-study treatments through shared source analysis and common processing, without first changing their sound or selection thresholds.
- Status: Provisional execution change; full-song profiling pending.
- Decision: Batch explicit listening-study jobs by source. Decode and measure the source once. Group jobs with identical finishing, tone, dynamic-band and compressor specifications and retain one exact oversampled processing prefix. Reuse final-drive renders only when the exact drive, ceiling and limiter timing match. Limit cached final probe PCM to 128 MiB per process. Keep per-job source, output, preservation, stereo, report and decoded-delivery checks independent.
- Reason: In the study track 1 pair, source preparation took about 0.05 seconds while six final drive probes repeated. Analysis reuse alone cannot deliver the requested speed gain; reuse must follow the actual shared render graph.
- Alternatives: Cache reports across changed jobs, reuse waveforms by approximate settings, skip final output checks, or copy previous masters into new versions. These would obscure changed processing or invalidate evidence. A persistent cache can follow after format, invalidation and storage rules are proven.
- Evidence: Shared-render measurement. Two ten-second study track 1 jobs fell from 14.083 to 8.455 seconds, with byte-identical WAVs and identical substantive JSON reports versus the frozen renderer. A normal batch runner and verified resume pass also succeeded. A full-length study track 1 pair matched historical hashes and reports in 136.29 seconds, versus 180.354 seconds summed from the earlier per-job reports.
- Validation state: Short-fixture and full-track identity/speed measured, including a changed-ceiling/limiter invalidation case; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --release` pass. Full-track peak resident memory was 1.23 GB for this pair; two-worker memory, contrasting material, broader cache effectiveness and cancellation remain to measure.
- Review condition: Any PCM or substantive report difference, excessive full-song memory, poor benefit on representative tracks, or changed output behavior after an interrupted batch. Cache keys, capacity and ordering should be revised from those measurements.
- Replaces: The prior per-job execution in the explicit treatment runner. No DSP or acceptance rule is replaced; [D033](#d033-render-independent-complete-chain-treatments-before-selecting-a-mastering-policy) remains the experiment design.

### Cache-budget update 2026-09-25

- Reduced the probe PCM budget from 256 to 128 MiB after the full study track 1 run. The only cross-job full-song cache hit was the first drive, which still fits. A repeated full-song run retained exact hashes/reports, took 136.29 seconds, and lowered reported peak memory footprint from 1.50 to 1.35 GB. This does not yet establish a safe browser memory budget.

### Scope correction 2026-09-25

- The user corrects the priority attributed above: shared analysis is a sensible
  architectural decision because one job produces multiple versions. Faster replay
  of the existing experiments is not the primary goal. D034's source-led automatic
  set remains the milestone. Preserve the measured cache results as supporting
  work; do not make further optimization a prerequisite to treatment generation.
- The active sequence is now first in the task list. This correction supersedes
  D037's original attribution of an immediate performance-first priority.

## D038: Bring source-derived treatment proposals into the native engine

- Recorded: 2026-09-25.
- Origin: D034's automatic multiple-version job and the user's correction that
  shared analysis supports this architecture; performance is not the primary goal.
- Status: Provisional proposal-stage implementation. Automatic set integration and
  broader source-only tonal/spatial/density families remain pending.
- Decision: Port the existing source-only spatial-overlap proposer to
  `source_treatments::propose`. Import the measured auditory diagnostics with original
  file identity, grid, activity, stem reconstruction and conversion checks. Keep
  unchanged processing, individual eligible regions, and a combined treatment within
  existing filter/response bounds. Record measured basis and analysis fingerprint.
- Reason: The positively received focus treatment of study track 1 has an existing source-derived
  basis that can be reproduced. The current `master_set` only explores conservative
  finishing families; optimizing it does not supply these missing treatment proposals.
- Alternatives: Copy the 680 Hz bell globally; promote track-e/track-h reference fits;
  invent fixed Warm/Bright/Wide slots; or first rewrite all auditory measurement in
  Rust. Reusing verified diagnostics makes the existing rule callable now while
  preserving the distinction between analysis, proposals, rendering and evaluation.
- Rules: `treatment.spatial-overlap.v1` carries over the measured-region thresholds,
  weighted centre, bounded Q and +3 dB trial from D032/D033. The amount and thresholds
  remain experimental. Individual-region isolation is an additional proposal for
  complete-chain comparison; no count is imposed on the eventual listening set.
- Evidence: Treatment study, including the preferred A/C of study track 1,
  the source-only spatial tests of track-e, and the lack of this opportunity in track-h. The
  preferred Full/Dense treatments of track-e involved reference fitting and are not reproduced
  by this one family. Native verification matches all
  saved source-only regions and filter settings.
- Validation: Seven regression tests cover direction/frequency dependence, unchanged
  and contrary cases, adjacency, counts beyond three, combined bounds, stale/malformed
  evidence, determinism and reference independence. Known-track proposal replay is
  measured separately from sound and preference. Required Rust gates are recorded
  with the verification run.
- Limits: Existing offline diagnostics are still required. This stage does not
  render, approve or describe completed masters and is not yet wired into `master set`.
  Separation leakage and local-versus-full-filter differences remain. Source identity
  checking does not establish the accuracy of individual separated estimates.
- Review condition: Complete-chain tests invalidate the predicted direction, listening
  shows the fixed probe amount is unhelpful, additional material has different needs,
  or proposal combinations create redundant or detrimental results. Calibrate from
  these observations before promoting acceptance rules.
- Work reference: task list section "Active focus: one recording, a useful set of masters".
- Extends: D034; corrects D037's priority attribution via its dated scope update.

### Integration update 2026-09-25

- D039 connects the proposal stage to completed native set rendering, measurement,
  immutable delivery and an audition page. This replaces the pending integration
  state above; the proposal thresholds remain provisional listening hypotheses.

- 2026-09-26 scope review: [D062](#d062-diagnose-full-range-all-stem-and-passage-coverage-before-judging-version-counts)
  records the track-f/track-e audit. Integrating this first family did not complete the
  intended full-range, all-stem search; global proposal medians can exclude local
  opportunities before passage refinement. Production thresholds remain unchanged.

## D039: Complete a native set job using the A–F treatment renderer

- Recorded: 2026-09-25.
- Origin: The user requests a finished working flow and clarifies that the older
  work predating the large A–F studies is not needed as its baseline.
- Status: Implemented native flow; musical calibration remains provisional.
- Decision: Replace the finishing-only set prototype with `set::render` and a
  per-version sink. Extract the A–F renderer into `treatment_render::Renderer`,
  used by both the study command and the new native job. Retain unchanged tonal/
  spatial processing, each source-derived focus region and the supported combined
  focus; render material-dependent base/density trials. Do not carry the older
  generic correction, widening or first-passing controller into this job.
- Decision: One immutable source analysis per job, one exact prepared prefix,
  and at most 128 MiB of shared probe PCM. Use shared immutable audio ownership
  to avoid duplicate cache copies. Persist each accepted version before rendering
  the next; native duplicate comparisons load one previously delivered WAV at a
  time. Sinks can stop at progress/delivery boundaries, preserving earlier outputs.
- Decision: Source 50 ms crest median supplies a bounded initial target (negative
  median, clamped −16…−8 LUFS). Median crest ≥6 dB and either crest P90−P10 ≥1 dB
  or ≥8 measured onsets supports a +1.5 LU density trial, capped at −6.5 LUFS.
  These are explicit starting heuristics motivated by retaining dynamic/denser
  interpretations from the study. They are not physical identities, standards,
  or a claim to reproduce manually chosen study delivery levels exactly.
- Decision: Keep the complete study DSP chain and independently remeasure its
  output. Nonfinite audio, changed length, missing loudness, reconstructed-peak
  failure or failed decoded delivery are technical faults. The study's source
  preservation costs remain measured diagnostics and do not reduce drive or veto
  a stronger interpretation automatically. Fixed −1 dBTP is this job's delivery
  ceiling; explicit study jobs retain their own requested ceilings.
- Decision: Compare completed waveforms against all delivered versions after best
  positive constant-gain matching. Residual below −40 dB suppresses a provisional
  near-duplicate. This replaces the prototype's −30 dB screen; it is deliberately
  more inclusive and still needs listening calibration. No fixed count, taste
  score or inverse research-control slot is imposed.
- Decision: Describe achieved gain-matched band changes, side/mid balance, limiter
  workload and delivered loudness. Bind source diagnostics to both original-file
  identity and the decoded engine PCM. Record target/timing rules and measured
  bases, every rendered trial, technical failure and duplicate comparison.
- Decision: `master set` writes immutable v2 sidecars, a final manifest, shared
  analysis/trials, append-only progress, an exact original-file copy and an
  audition page. Browser playback defaults to matched loudness; delivered playback
  uses unity gain. Keep playback position when changing versions. Saved choices
  are bound to the exact output hashes. Existing outputs are never replaced.
- Alternatives: Continue extending the earlier finishing-only selector; repeat
  separate CLI runs with preset intents; accumulate every full output in RAM;
  return proposals without audio; or promote the track-e/track-h reference fits to
  global curves. These do not finish the agreed source-led, one-job listening flow.
- Evidence: D033/D034 and initial A–F listening feedback; D038's source-proposal
  parity; byte-identical extraction replay of two study WAVs of study track 1 with unchanged
  substantive reports; focused native set/cache/invalidation tests; full-song and
  browser results in the native set run.
- Limits: The current source-only treatment family is spatial-overlap focus plus
  density. Reference-derived Full treatments are still calibration experiments.
  Existing auditory diagnostics are optional backend input; generating all those
  measurements natively and replacing the main product worker are further work.
  Full-song native completion does not establish browser render-memory suitability
  or listener preference for every generated version.
- Review condition: Listening finds a useful suppressed treatment or an unhelpful
  retained result, source-crest targets overprocess particular material, full-chain
  spatial changes contradict the proposal, or new material requires another family.
- Supersedes: D035/D036's finishing-only native set and v1 delivery contracts.
  `pipeline::master_set` and its set-only helper types are removed; no compatibility
  path remains. Single-master DSP research history is retained separately.
- Extends: D034, D037's shared-work architecture and D038's source-derived proposals.
- Work reference: TODO "Native one-job flow" and "Listening and broader treatment work".

### 2026-09-25: Name the achieved result in each recording

- Origin: The user asks whether names vary with the shape of the audio.
- Implemented: `set.result-name.v1` derives display names from completed, matched
  band and stereo measurements and actual limiter workload. A proposed filter or
  higher requested target alone cannot earn its intended name. Essential remains
  the unchanged tonal/spatial anchor. Stable IDs remain separate; filenames include
  both names and IDs, with bounded UTF-8 prefixes for long labels/track names.
- Reason: Names and descriptions help choose after listening and must describe
  this recording's result. A fixed preset menu or filter-frequency label does not.
- Evidence: Source-focus/density regression checks assert measured centre-body
  and density names; full-song names and browser association checks are recorded
  in the native set run. Naming does not alter processing or dither seeds.
- Limits/review: The description thresholds in RULES.md are provisional; revise
  if listening contradicts a label or materially different versions earn confusing
  names. Names are neither preference rankings nor guarantees of audibility.

### 2026-09-26: Completed native delivery and listening verification

- The final native jobs delivered 4 masters for study track 1, 8 for track-e, and 2 for
  track-h. All original files and exact audition copies retained their hashes.
- All 14 masters passed decoded delivery and independent FFmpeg checks. Browser
  checks covered all 17 selectable files, actual playing PCM/gain, names, switching,
  failed-load recovery, downloads and saved choices. Desktop/mobile layouts were
  checked, including track-e's longer combined-treatment names.
- The final Rust gates passed: formatting, strict Clippy, and 141 release tests.
  The native set run records exact identities,
  measurements, resource observations and retained preservation diagnostics.
- This completes the native focus/density job and audition flow. Broader treatment
  families, listening calibration, unfamiliar material and main-product worker
  integration remain distinct work; numerical completion does not establish taste.

### 2026-09-26: Positive listening feedback on the completed sets

- The user reports: “these are all good”, with no criticism, and requests further
  refinement toward bespoke treatment for each track, fuller use of stems, and
  an account of additional-version processing time.
- This is positive listening evidence for the supplied study track 1, track-e and track-h
  sets in the second automatic-set build. Preserve those exact outputs as the next
  comparison baseline. No individual ranking, playback mode, monitoring level or
  listening conditions were supplied; do not infer those or generalise the feedback
  to unfamiliar material.
- [D040](#d040-refine-treatment-amount-timing-and-passage-specific-evidence) records
  the proposed refinement direction. This feedback does not by itself validate
  every provisional threshold or change implemented processing.

## D040: Refine treatment amount, timing and passage-specific evidence

- Recorded: 2026-09-26.
- Origin: Positive feedback on all current automatic sets and a request for more
  bespoke decisions, fuller use of stems and additional-version timing information.
- Status at proposal: Proposed refinement sequence; no new DSP/controller policy
  implemented at that point. See the dated implementation update below.
- Current evidence: The delivered sets used source-bound auditory stem diagnostics
  for vocal/accompaniment spatial-overlap regions. Centres, spans and domains vary
  by source. Focus gain remains +3 dB; the eligible denser target is base +1.5 LU.
  All 14 final reports used 50 ms limiter release, and their separate `stem_evidence`
  field is null. The native drum-decay/bass-body timing path exists but was not
  supplied to these jobs; its earlier heuristic is not newly validated by this review.
- Proposal: First refine treatment amount and density spacing from the particular
  recording's measured response, while keeping the liked outputs as controls.
  Evaluate stem-informed peak recovery alongside the current timing. Then preserve
  passage-level evidence to distinguish persistent opportunities from intermittent
  ones and support bounded dynamic treatments where warranted.
- Reason: The current source-led family identifies where a treatment may help.
  More track-specific decisions also require evidence for how much change and when
  it is useful. Stems remain imperfect analysis estimates; output processing stays
  on the original stereo mix. Global reconstruction checks alone do not establish
  reliable instrument isolation in every region or passage.
- Proposal: Add per-stage/per-version elapsed-work reporting and paired same-source
  runs to measure marginal version cost. Current completed-job time divided by
  count is an average including shared overhead, not a measured incremental cost.
  Existing stem preparation is outside those job timings and need not be repeated
  for each version.
- Alternatives: Add fixed preset amounts or a version quota; enable old stem
  heuristics without renewed comparison; or infer musical quality solely from a
  spectral or overlap score. These would not establish the requested refinement.
- Validation: Source binding and unreliable/local evidence cases; deterministic
  proposal/render regression tests; complete-chain measurement and delivery checks;
  matched and delivered listening against the liked baseline; unfamiliar contrasting
  recordings. Keep useful strong and subtle interpretations eligible.
- Review conditions: A proposed refinement is indistinguishable, damages a liked
  quality, reacts to separator leakage, flattens section contrast, or incurs extra
  renders without useful choice. Revise or omit it on that evidence.
- Work reference: TODO "Listening and broader treatment work";
  current measured results and timing.

### 2026-09-26: Implement the three source-specific refinement stages

- Implemented: `refinement::analyze_with_reconstruction`, source/stem-bound native
  analysis, measured strength, stem recovery and passage envelopes. `master set`
  connects these automatically when aligned raw stems accompany the auditory region
  hypotheses. Rendering continues to use only the original stereo mix.
- Strength decision: Probe actual candidate EQ/bandpass responses at 0…6 dB in
  0.75 dB steps, using reliable vocal-overlap passages. With ten eligible seconds
  and ≥0.1 dB maximum median ratio response, interpolate the first gain reaching
  80% of that maximum, then measure the selected filter. Insufficient evidence keeps
  the study strength and records the limitation. This is not a vocal-quality score
  or a claim about the nonlinear completed master. Combined gains retain the 9 dB
  response bound with a recorded common scale when necessary.
- Density decision: Replace the fixed +1.5 LU interval with half the source's
  P90−P10 50 ms crest spread, bounded 0.5–3 LU and the existing −6.5 LUFS target cap.
  Keep the previous base-target and eligibility rule. This remains provisional.
- Timing decision: Use measured six-dB drum decay to propose an exponential recovery
  constant, bounded by bass period, event interval and 25–160 ms; retain 50 ms when
  unreliable. The bass floor wins if the spacing cap conflicts. Exact formula and
  activity/reliability requirements are in RULES.md. Do not promote the older
  half-decay heuristic merely because it existed.
- Passage decision: Use reliable two-second observations to reduce the focus
  contribution outside supported overlap passages. Use continuous filter state and
  slow raised-cosine wet blending. Require meaningful measured variation before
  enabling movement; no artificial verse/chorus labels or whole-mix gain riding.
- Architecture: Reuse the validated resampled stem sum and convert the vocal once,
  avoiding four repeated conversions. Bind reconstructed PCM with its own hash.
  Fingerprint passage envelopes in prefix reuse. Complete per-output verification,
  measured descriptions and source preservation remain required.
- Timing evidence: Record job preparation, elapsed progress and per-version render/
  measurement/comparison and WAV write/verify/hash costs. These operational times
  remain separate from deterministic renderer evidence and do not claim a speedup.
- Alternatives: Keep shared 3 dB/1.5 LU/50 ms settings; extrapolate the local +3 dB
  sensitivity as though it were a rendered filter; retune recursive coefficients
  abruptly; or render separated stems. The selected approach supplies measured
  source-specific decisions while preserving the stereo mastering boundary.
- Validation: Regression checks cover material-dependent strength and timing,
  exact dry passages, local corruption despite acceptable global reconstruction,
  stale evidence, malformed envelopes and cache invalidation. Real-song evaluation
  and unchanged A–F replay are recorded in the refinement run.
  All 146 release tests, formatting and strict Clippy pass. Three complete jobs
  deliver 14 independently verified masters; actual browser PCM/gain checks pass
  for those and their three original copies. All previous masters remain unchanged.
  These are numerical and interface checks; listening calibration remains open.
- Limits: Stem leakage may survive reconstruction checks. Two-second measurements
  and before-limiter filter response do not prove audibility or listener preference.
  The 80% response criterion, timing map and passage thresholds require listening
  calibration and unfamiliar-material counterexamples. Retaining an existing
  strength for insufficient evidence is explicit fallback, not a new validation.
- Supersedes: D039's fixed denser interval and, where complete refinement evidence
  exists, its shared focus amount and fixed release. Unchanged processing remains
  eligible; no version-count policy is changed by this implementation.

## D041: Measure job resources and stage alpha deployment

- Recorded: 2026-09-26.
- Origin: Request for initial/incremental/cumulative version costs, memory growth,
  WASM feasibility, parallel Lambda execution and a route to an alpha.
- Status: Resource measurement implemented; deployment architecture proposed.
- Measurement decision: Retain elapsed version boundaries and add an external
  process-RSS sampler. Distinguish sampled phase peaks, cumulative peaks, completion
  residency and output storage. A previous whole-job peak cannot establish growth
  per version. Sampling is operational evidence, not a DSP or selection input.
- Implemented: The set profiler launches a new immutable native set and
  records a timed RSS trace plus phase summaries. Existing audio is preserved.
  No treatment, density, naming or version-count rule changes.
- Portability evaluation: Compile the engine for wasm32 and run a temporary direct
  set-core probe, separately from the older single-master wrapper. Keep short
  synthetic success distinct from a supported full-song browser product. Pin
  matching wasm-bindgen tool/library versions for future reproducible builds.
- Proposed initial deployment: Queued native rendering with durable source/output
  storage and browser progress/audition. Benchmark the intended Linux architecture,
  memory allocation and real source preparation before promising latency or cost.
- Proposed parallel boundary: Persist a source-bound plan, execute independent
  treatment families, and keep base/denser variants together to retain prefix reuse.
  Finalize cross-family comparisons and names in stable order. The current
  sequential set API does not yet expose that distributed boundary.
- Alternatives: Full-set standard Lambda invocation; separate invocation for every
  version; browser-only execution; an existing native worker service or managed
  containers. Whole-job duration, repeated preparation, browser copies/storage and
  concurrent memory make these materially different choices.
- Evidence and limits: Alpha deployment assessment records
  local timings, memory measurements, portability checks and current official
  platform limits. Parallel timing arithmetic is a model, not a cloud benchmark.
  No backend is deployed and no full-song browser readiness is claimed.
- Validation/review: Full-song resource curves; unchanged output fingerprints;
  device/architecture parity; cancellation/retry/partial-delivery behavior; measured
  P50/P95 time to first and all masters; total worker-seconds, storage and transfer.
  Revisit the execution choice when those measurements or alpha requirements differ.
- Work reference: TODO "Alpha deployment"; operations in README.md.

## D042: Bounded peak control and SoundKit I/O

- Recorded: 2026-09-26.
- Origin: Reduce the measured 1.3 GiB memory cost and preserve the same masters.
  Use SoundKit for streaming audio I/O.
- Status: Accepted architecture; implemented first reduction. Complete bounded
  job memory remains planned work.
- DSP decision: Preserve the existing forward release and reverse attack exactly.
  Store their boundary states for blocks of 16,384 oversampled frames.
  Reconstruct block gains and retain filter overlap for downsampling.
  Keep sample rounding and statistics in their original order.
- Reason: The previous probe duplicated the complete oversampled signal and stored
  a complete gain envelope. Neither allocation is necessary for identical output.
- I/O decision: SoundKit owns WAV parsing, sample conversion, and container writing.
  The engine adapter reads 64 KiB byte chunks and writes 8,192-frame blocks.
  Engine policy controls stereo input, supported rates, finite samples, dither,
  minimum duration, and refusal to replace files.
- SoundKit changes: Add PCM24 streaming output, float `fact` chunks, RIFF padding,
  and explicit stream completion checks. Validate extensible formats and geometry.
  Pin the dependency to a published Git revision.
- Alternatives: Independent audio windows would reset state and change results.
  A finite reverse-attack horizon would also change results. Recomputing the
  oversampled prefix for every probe would increase repeated work.
- Retained allocations: Source PCM, one prepared oversampled prefix, the bounded
  probe cache, delivery verification PCM, and available stem analysis buffers.
  This implementation does not establish constant memory for a complete job.
- Verification: Boundary tests compare every float bit and all limiter statistics.
  The A–F replay preserves WAV hashes and substantive reports.
  Full-track measurements and release checks are in the memory run.
- Review conditions: Any coefficient, ordering, codec, block, cache, or storage change
  requires equality checks. Consider bounded prefix storage and sequential stem
  analysis when the next resource profile identifies the largest remaining cost.
- Work reference: TODO "Memory reduction". D041 deployment options remain proposals.

### Updates

- 2026-09-26: All 14 full-song WAVs and substantive reports match the saved refined sets.
  track-e's sampled peak decreases from 1.293 to 0.948 GiB. Concurrent load affects timing and residency.
  The short WASM probe decreases from 52.69 to 37.50 MiB with identical passage measurements.
  Formatting, strict Clippy, 148 release tests, and the WASM target check pass.
  SoundKit streaming changes are published and pinned at `088ab967d2ec4dbdb6330b6675c76bad5055e756`.

### Further update

- 2026-09-26: [D043](#d043-seekable-scratch-storage-and-sequential-stems) replaces the retained in-memory prefix and probe cache.
  The first checkpoint measurements remain preserved.

## D043: Seekable scratch storage and sequential stems

- Recorded: 2026-09-26.
- Origin: The user rejected the remaining 0.95 GiB footprint after D042.
  A roughly 70 MB recording should not require twenty times its file size in RAM.
- Status: Accepted and implemented for native sets and treatment rendering.
  Complete bounded job memory and product browser integration remain planned.
- Storage decision: Generate the shared 4× prefix in blocks and store exact float32 PCM in seekable scratch storage.
  Reuse that prefix across the existing limiter passes and density versions.
  Cache matching final probes in scratch storage. Keep the best search candidate there until selection.
- Host interface: `scratch::Factory` supplies independent `Read + Write + Seek` handles.
  Native defaults use temporary files. Unix files have owner-only permissions and are unlinked after opening.
  Non-Unix native handles close and remove their files on drop.
  WASM defaults use memory; browser hosts can supply OPFS adapters.
- Precision decision: Retain float32 PCM and the existing f64 calculations.
  PCM24 is a storage and delivery format. Converting intermediates to PCM24 would add rounding and restrict headroom.
  Float32 uses one-third more sample storage than PCM24; retained copies caused the much larger footprint.
- Stem decision: Load, fingerprint, and measure drums, bass, other, and vocals sequentially.
  Preserve the exact original sum order. Retain vocals for refinement and reuse reconstruction as accompaniment.
  Consume owned equal-rate PCM without cloning. Always render the original mix.
- Reason: The shared prefix and saved probes need random access, but they need not remain resident in the process heap.
  The noncausal limiter keeps its exact global passes, state, coefficients, rounding, and statistics.
- Alternatives: Recompute prefixes for every probe; quantize scratch PCM; keep smaller in-memory caches; approximate independent processing windows.
  These increase repeated work, change samples, or retain the largest allocation.
- Tradeoffs: Additional disk reads and writes, temporary storage capacity, and host error handling.
  File caches still use reclaimable system memory. RSS reduction does not establish container memory or cloud billing.
  Full source, reconstruction, resampling, current output, and verification buffers remain.
- Verification: Exact scratch bits and colour statistics; block limiter equality; sequential stem/reconstruction equality at 44.1 and 48 kHz.
  Full-song identity and resource results are recorded in the scratch storage run.
  The short browser probe compares memory and OPFS PCM fingerprints and verifies the saved passage measurements.
- Review conditions: Repeat equality checks after storage, block, coefficient, ordering, cache, or codec changes.
  Measure actual disk capacity, I/O, memory accounting, cleanup, and latency on each deployment host.
  Validate full-song browser storage, cancellation, and device limits before making product guarantees.
- History: Supersedes D042's retained in-memory prefix and probe-cache implementation.
  D042's exact limiter blocks and SoundKit ownership remain active.
- Work reference: TODO "Memory reduction"; native operations in README.md.

### Updates

- 2026-09-26: All 14 WAVs, original copies, notes, and substantive reports remain identical.
  track-e's sampled peak decreases from 971.06 to 386.55 MiB.
  Formatting, strict Clippy, 150 release tests, and WASM checks pass.
  The six-second OPFS probe uses 12.94 MiB of WASM linear memory and preserves the memory-adapter PCM hashes.

## D044: Local mastering comparison sets

- Recorded: 2026-09-26.
- Origin: The user requested a simple MASTER screen centred on listening to the original and automatic versions, with no prior tone or finish questionnaire.
- Status: Accepted and implemented for the local browser. Listening review remains under validation.
- Implementation: The browser worker streams SoundKit float PCM into private OPFS input files, calls `mastering-wasm::render_set`, and supplies OPFS scratch to the deterministic Rust renderer. The source and completed PCM24 WAVs occupy independent ranges in an append-only bundle. The screen presents the original and each engine-named version with its measured description, one aligned player, matched/delivered level choice, download, report, and add-to-Tracks actions.
- Reason: The source and render measurements determine worthwhile alternatives and descriptions. The user decides by listening. A source must be preserved and completed versions must remain useful if later rendering fails.
- Alternatives: The earlier single-master finish/tone form and record-deck comparison obscured the automatic set and duplicated the main app's player. A cloud job would add an unnecessary dependency for the local prototype.
- Evidence: User product direction and the existing `set::render` rules. The full headless MASTER flow passes. The 233-second track-e persistent-browser run delivered eight PCM24 WAVs whose SHA-256 hashes all match native; total worker time was 589.05 s versus 489.22 s native including preparation (20.4% slower in unpaired runs). See the browser run record.
- Rules: No user stereo-width control. No fixed version count. Retain unchanged candidates. Stems may guide decisions; always render the source stereo mix. Without source-bound auditory evidence, current sets expose the density family only; the interface must not invent tonal variants.
- Limits: Source and output PCM, verification, and playback decoding still scale with song length. The full browser run reported 420.44 MiB of WASM linear memory and 1.98 GB of OPFS use before scratch cleanup. A temporary Playwright context returned Chromium `FILE_ERROR_NO_SPACE`; persistent-profile storage completed the set. Mobile device memory and listening quality remain unverified. No historical set migration is supplied for the replaced prototype contract.
- Verification: Stream/file WAV parity for float32, PCM16 and PCM24 plus nonfinite rejection pass. The headless app test passes import, source preservation, real WASM render, per-version notes, audible aligned playback, matched gain, downloads, add-to-Tracks, reload, cancellation, invalid evidence, and four mobile/desktop palette layouts. Full-song numerical evidence belongs in the browser run record. Listening review remains open.
- Review condition: Revisit level matching, candidate descriptions, set retention, and resource limits after real-music listening and desktop/mobile full-song runs.
- Supersedes: The app's single-master beta interface contract; D043 scratch policy remains active.
- 2026-09-26 update: Safari rejected structured cloning of an OPFS directory handle into the render worker. The worker now reopens the job using cloneable library, track, and job identifiers; an alternate in-memory output path was rejected because it would increase whole-song memory. The complete original WAV is delivered before optional stem analysis so listening can start while versions render. Within a version, global loudness search and the backward limiter envelope still require full-song passes; verified versions remain the publication boundary. WebKit and Chromium headless page flows pass with a guard against sending directory handles. Revisit chunk-level playback only if measured waiting time after final render is substantial and a provisional-audio contract is approved.
- 2026-09-26 update: [D045](#d045-shared-local-auditory-and-stem-preparation) replaces the optional-evidence assumption for normal MASTER jobs. D044's no-implicit-split and no-cloud-dependency statements no longer describe that product flow; its renderer, preservation, comparison, and delivery decisions remain active.

## D045: Shared local auditory and stem preparation

- Recorded: 2026-09-26.
- Origin: A real track-f MASTER job delivered two versions while the best native track-e set delivered eight. The user requires the browser to use exactly the best native engine, including analysis and stem evidence.
- Status: Implemented. Full-song automatic native/WASM WAV equality and real-music listening remain under validation; short automatic byte parity passes.
- Decision: Both native `master set --stems` and browser `mastering-wasm` run the same Rust stem evidence, auditory measurement, `treatment.spatial-overlap.v1` proposal, source refinement, render, duplicate screen, and delivery rules. Explicit existing auditory JSON remains available for study replay and takes precedence. Delivery dither is seeded from verified decoded source PCM and version ID in shared Rust, so the same samples delivered as WAV or SoundKit produce the same PCM24 bytes. The app's MAKE MASTERS action uses four finished library stems or invokes its existing stemsplits path and waits for the four imports; incomplete/failed splits stop the job. Stems inform bounded decisions, while every version renders the unchanged original stereo mix.
- Reason: The former browser call omitted the external auditory artifact used by the best native runs. Merely sharing the final Rust DSP still gave a narrower decision path; choosing a version count from track complexity would conceal the missing measurements. An automatic native/WASM hash check also found that file-byte-seeded dither diverged when the same PCM entered as WAV versus SoundKit, despite identical DSP.
- Alternatives: Browser-only tonal presets, additional density slots, inferred focus from coarse 29-band stem evidence, or accepting two versions despite available aligned stems. These would create a different class of processing or bypass the study's measured policy. Embedding Python/SciPy in the browser would break the shared local Rust path.
- Evidence: The local Rust 4096-Hann/1024-hop, 64-roex-filter analysis of the saved track-e source and aligned stems reproduces the existing side regions 07–11 and 14–21, combined proposal, centre frequencies 334.6479586485557 and 604.8019614161277 Hz, and predicted gain ranges to numerical precision against the saved track-e analysis. The same source/stem sum passes −34.34 dB residual. It also reproduces study track 1's mid region 09–30 at 679.6026687186676 Hz and track-h's unchanged-only result. track-f's locally separated stems pass −31.16 dB residual; both Rust and the original Python script find no eligible focus region. Synthetic mid-vocal/side-accompaniment regressions produce mid focus and retain unchanged tone. Chromium and WebKit complete real WASM MASTER page flows with the new analyzer and match native PCM24 WAV hashes for both a one-version unchanged case and a two-version positive-focus case. Final `cargo fmt --check`, strict Clippy, release tests, WASM build and app web-performance check pass.
- Limits: Stem reconstruction proves only alignment/gain, not separation quality or listener preference. Complex material does not guarantee more eligible regions or retained versions. The app's stemsplits path may need sign-in and remote separation, while mastering/analysis stays local. Python and Rust floating-point roundoff can slightly change reported parameters; native and WASM now share Rust computations. Switching the dither seed changes PCM24 bytes relative to earlier saved output, without changing pre-delivery DSP; earlier byte-exact benchmarks document the old build. Full-song auto-analysis byte equality and analysis time are not yet measured.
- Review conditions: The new Rust measurement disagrees with further saved Python fixtures near a proposal threshold; track-f's completed stem analysis or listening rejects a suggested focus; a full native/WASM automatic set differs in decoded or delivered WAV bytes; separation capacity or waiting time makes implicit preparation unsuitable.
- Supersedes: D038/D039's assumption that source-bound auditory regions must enter the complete set through an external artifact, and D044's optional-stems product behavior. Their measured policy, imported-artifact validation and complete-render evaluation remain active.
- Work reference: TODO "Local browser mastering" and the browser run record.

## D046: Parallel treatment-family execution

- Recorded: 2026-09-26.
- Origin: The user asked whether mastering could use Lambda parallelism like stem separation, how to divide it, and requested a spike.
- Status: Local native feasibility spike completed; the distributed execution boundary is proposed, not accepted or deployed. The current set path remains sequential and the browser remains local WASM.
- Proposal: Run current source/stem/auditory preparation once, persist a source-bound ordered plan, and render each complete treatment family in an isolated native worker. Keep a family's base and denser versions together for prepared-prefix reuse. Finalize candidate comparison, naming and immutable delivery in stable plan order, regardless of worker completion order. Each worker renders the original complete stereo mix.
- Reason: Family render specs share input analysis but have no inter-family DSP state. Time slices do carry filter, limiter and measurement dependencies. An isolated worker can use the same `treatment_render::Renderer` without inventing a second mastering algorithm.
- Alternatives: Monolithic local WASM (current product behavior); a single native cloud job; one invocation per version (forgoes within-family cache); arbitrary song-length chunks with overlap (needs a new exactness design); persistent native workers/containers instead of Lambda. Select a host only after complete-song latency, cost and parity measurements.
- Evidence: The parallel family spike replays track-e's recorded eight-version plan. Four local family threads take 107.64 s versus 341.98 s serially for the full 232.91-second recording (3.18×), and 11.62 s versus 43.29 s for its first 30 seconds (3.73×). All eight raw PCM SHA-256 hashes and selected drive bits match in both runs; each family reuses its prefix once. The full source PCM hash matches the recorded plan. Independent no-overlap time slices change 787 channel samples on the 30-second excerpt. With an experimental global peak budget, a separate uneven synthetic source differs in 2,325 channel samples even with two seconds of overlap. These timings exclude shared preparation, delivery and cloud overhead.
- Limits: A recorded plan replay is narrower than a current automatic end-to-end set. macOS threads are not Lambda. Separate workers replicate source reads/scratch, and native Linux/WASM cross-target parity is untested. AWS's 900-second invocation limit requires per-family measurement on the target configuration. No finalizer, retry protocol or cloud upload contract is implemented.
- Validation/review: Extract plan/execute/finalize without changing `set::render` decisions or PCM24 delivery, then compare full actual sets under out-of-order completion. Measure Linux worker memory, scratch, transfer, first/all-version latency and cost on the intended host. Revisit Lambda versus a persistent worker if any family approaches timeout or overhead dominates.
- Extends: [D041](#d041-measure-job-resources-and-stage-alpha-deployment), [D045](#d045-shared-local-auditory-and-stem-preparation). No active DSP or product rule changes.
- Work reference: TODO "Alpha deployment" and the parallel family spike record.

### Updates

- 2026-09-26: [D047](#d047-private-arm64-lambda-family-test) accepts and implements
  the source-bound plan and isolated family worker for a private test. Stable
  cross-family finalization and browser cloud mode remain proposals. The earlier
  "not deployed" status describes the local spike before this test.

## D047: Private ARM64 Lambda family test

- Recorded: 2026-09-26.
- Origin: The user asked to test parallel mastering on Lambda like the existing
  encoder, then selected US East 1 with low test cost as the priority.
- Status: Accepted and implemented for a private test. Product cloud delivery
  is not implemented.
- Decision: `master set-plan` uses the normal Rust source and aligned-stem
  analysis. `set::render` consumes the same ordered family plan. Each ARM64
  Lambda receives the original stereo mix and source-bound plan from a private
  S3 bucket. It runs the shared `distributed::render_family` Rust function,
  verifies PCM24 delivery, and writes unique WAV objects without replacement.
  Direct IAM invocation avoids an API Gateway and idle service. The test stack
  uses four reserved invocations, one-day S3 and log retention, and one ECR
  image. The browser MASTER screen continues to use local WASM.
- Reason: Whole treatment families are independent after shared planning.
  Keeping each base and denser pair together retains renderer reuse. One full
  song per worker keeps global loudness, filtering and limiter state intact.
  A private, single-region test measures actual cloud cost and parity with
  little fixed infrastructure.
- Alternatives: EU-region test, a single long Lambda, per-version invocations,
  Step Functions, API Gateway, and a persistent container. This test selects
  US East 1 and direct invocation for low setup and run cost. It does not settle
  production placement or regional user latency.
- Evidence: The Lambda measurement records a 49.49 s
  repeat plan, about 61.17 s source upload and setup, and 184.43 s parallel
  invocation wall time for the 232.91 s track-e source. All eight PCM24 WAV hashes
  match local native family renders and a fresh normal `master set --stems`
  run. The smoke WAV hash also matches. The full native set took 431.32 s;
  the partial cloud path took about 295 s with local planning and upload. Worker
  peaks are 888–917 MB. CloudWatch billed 676.910 worker-seconds at 1,769 MB,
  or about $0.0156 ARM64 compute before free tier at the cited rate.
- Limits: This is one real song and one short synthetic smoke test. Planning
  occurs locally with existing aligned stems. It does not call stemsplits or
  finish cross-family duplicate checks, measured names, notes, and publication
  on Lambda. Client upload, storage, extra scratch, requests, and downloads are
  separate costs. Linux/macOS selected-drive and loudness floats differ by up
  to 1.8e-14 dB and 1.8e-15 LUFS, despite identical delivered WAV bytes.
  The result is not a P50 or P95 service estimate.
- Verification: Full-song Linux ARM64 versus macOS normal native-set WAV hash
  parity passes for all eight versions. The normal set's original copy matches
  the input file. Formatting, strict Clippy, and all release tests pass.
- Review conditions: Revisit placement after full set finalization, more real
  tracks, current native/WASM/cloud parity, browser upload timing, memory and
  scratch limits, retries, and total service cost are measured.
- Extends: [D046](#d046-parallel-treatment-family-execution). No DSP or
  mastering policy changes.
- Work reference: TODO "Alpha deployment" and the Lambda measurement record.

## D048: Signed-in Lambda MASTER with explicit local processing

- Recorded: 2026-09-26.
- Origin: The user asked for the MASTER screen to use Lambda unless local processing is selected, with sign-in required for cloud processing.
- Status: Accepted and implemented for the single-region alpha; listening and broader device/track validation remain open.
- Decision: The Account local-processing switch also controls MASTER. Off selects a signed-in US East 1 Lambda job; on retains local WASM. WASM performs the same source and aligned-stem analysis and produces the exact source-bound family plan. The browser uploads only that plan and a float32 WAV of the original stereo mix to private, one-day S3 objects through short-lived, create-only URLs. Four ARM64 family Lambdas render whole-song candidates in parallel using the native Rust engine. A finalizer applies the same gain-matched duplicate threshold and measured naming/description functions in stable plan order. Verified PCM24 versions are downloaded as new local files; the source comparison copy remains on the device. The Cloudflare service validates a verified identity session before the control Lambda can create or read an owner-bound job.
- Reason: Source-led stem analysis determines treatments, while every render still processes the untouched original stereo mix. Whole-family jobs preserve global DSP state and within-family reuse. The existing explicit local preference gives the user a device-only path.
- Alternatives: A browser-only default, uploading all four stems to Lambda, a single serial Lambda, and a public unsigned job API. The selected plan keeps expensive stem analysis local, reuses the measured parallel family boundary, and requires the existing identity session. It does not claim the whole cloud path is faster until upload, finalization, and downloads are measured.
- Evidence: [D047](#d047-private-arm64-lambda-family-test) measured eight track-e PCM24 outputs matching native byte-for-byte and approximately 295 seconds through planning, upload, and family renders, before finalization or downloads. The finalizer regression compares names, notes, and retained versions with `set::render` on source-bound fixtures. The completed cloud run checks the full eight-version track-e set, live browser flow, and downloads.
- Limits: This is a single-region alpha with a five-minute source limit and one-day cloud object retention. Stem splitting remains in stemsplits and can be triggered before a cloud plan. Local-only mode with no saved stems uses the same set engine without optional stem evidence and says so on screen; it never silently starts a cloud split. Cancelling the screen stops client work; a submitted Lambda job can finish and be resumed. Real signed-in browser testing through the deployed identity Worker, failure/retry behavior, and broader track/device measurement remain before a production service commitment.
- Verification: Local WASM browser regression, cloud browser flow with a synthetic verified session and real S3/Lambda transfer, live unsigned edge check, native full-set parity, and strict Rust checks pass.
- Review conditions: Revisit region, upload/download throughput, complete-job time and cost, peak finalizer disk/memory, partial retries, and real-song listening results.
- Extends: [D047](#d047-private-arm64-lambda-family-test). No DSP or treatment threshold changes.
- Work reference: TODO "Alpha deployment", README operations, and the Lambda measurement record.

### Updates

- 2026-09-26: D047 remains the measurement of the earlier private family test; D048 adds signed-in control, finalization, and browser selection without changing those measured worker results.
- 2026-09-26: A four-second real cloud browser flow produced a native-identical WAV. The full track-e control job finalized eight byte-identical native versions with the same names and notes; eight sequential downloads all matched their hashes. The finalizer's 1,750 MB peak at a 1,769 MB setting prompted a separate 3,008 MB finalizer setting; the family memory setting stays 1,769 MB. The new setting is deployed, and a full-song rerun at that setting remains a review condition. The `mstr.yl.vin` custom domain redirects to the existing `yl.vin` Tracks/Master route because both the device library and sign-in cookie are host-bound; the API remains same-origin on `yl.vin`. See the run and the app's MASTER contract.
- 2026-09-26: [D049](#d049-rust-only-mstr-api-and-zeroth-aws-authorizer) replaces the Cloudflare mastering proxy, shared secret, Function URL, and redirect recorded above. The earlier run remains evidence for the same native family DSP and output hashes.

## D049: Rust-only MSTR API and Zeroth AWS authorizer

- Recorded: 2026-09-26.
- Origin: The user designated `mstr.yl.vin` for the AWS API, requested AWS-side Zeroth authentication that can be distributed, named service tokens `MSTR`, and required Rust for every deployed MSTR backend component.
- Status: Accepted. The regional API Gateway, Zeroth Rust authorizer, direct browser token flow, and Rust control Lambda replace the previous proxy and Function URL. Real signed-in user validation remains open.
- Decision: `mstr.yl.vin` is a DNS-only CNAME to the US East 1 API Gateway custom domain. The Rust `yl.vin/id/tokens` Worker uses the host-only browser session to mint a 300-second ES256 token for the `yl-web` client and fixed `MSTR` audience. The browser sends this bearer token directly to the API. A reusable `zeroth-aws-authorizer` crate in the Zeroth workspace verifies its signature with Zeroth's public JWKS plus exact issuer, audience, client ID and lifetime inside AWS. API Gateway caches no authorization result and allows CORS only from `https://yl.vin`. The Rust control Lambda takes the signed subject solely from API Gateway authorizer context and owns one-day S3 job objects and create-only upload/download URLs. The mastering DSP, family workers and finalizer remain the same native Rust engine from D048.
- Reason: AWS can independently reject untrusted requests without a shared bearer secret or a Cloudflare mastering proxy. The first-party token mint preserves the existing host-only session, while the direct API hostname is usable by other Zeroth clients with their own configured audience policy. Keeping the authorizer in Zeroth makes the security code distributable and reusable.
- Alternatives: API Gateway's built-in JWT authorizer, which does not accept Zeroth's ES256 tokens; a shared Worker-to-Lambda secret; a Python or JavaScript Lambda authorizer/control service; and proxying the API behind Cloudflare. The chosen Rust path reuses Zeroth's tested verifier and keeps the control plane in the engine repo.
- Evidence: Local signed-token regressions accept `MSTR` and reject changed audience, client, issue time and expiry. A live signed test token passed the Rust AWS authorizer; a different audience received HTTP 403. The direct Chromium flow minted a fixture token, used API Gateway and real S3/Lambdas, and received the native-identical Essential WAV hash `490645200a3b46f24ca4efdf383ef21e12fc79cbb6bd8eb41270bf71bb4cb254`. API preflight admitted `https://yl.vin` with `Authorization` and `Content-Type`. The full-song parity measurements remain in the cloud run.
- Limits: A minted token remains valid for up to 300 seconds after browser session revocation. API Gateway throttling is global; account-scoped job throttling and abuse testing remain open. The headless browser uses a signed fixture token, not a real signed-in person's session. DNS, IAM, and CORS are configured for the US East 1 test deployment only.
- Verification: Zeroth authorizer and Rust control unit tests, live valid/invalid API tokens, unsigned API denial, direct headless browser cloud flow, root Rust formatting/Clippy/release tests, and deployment configuration inspection. A full-song rerun after this authorization change is not required for DSP parity because family and finalizer binaries are unchanged, but remains useful for capacity assessment.
- Review conditions: Test a real signed-in browser through Zeroth issuance; measure key rotation and revocation behavior, job abuse and per-account throttling; revisit region and service capacity before opening production traffic.
- Supersedes: D048's Cloudflare proxy and `mstr.yl.vin` redirect contract, while retaining D048's source-bound plan, native family render, finalizer, and local WASM fallback.
- Work reference: TODO "Alpha deployment", the Zeroth AWS authorizer crate (`crates/zeroth-aws-authorizer`), the engine AWS deployment, and the app's MASTER contract.

### Updates

- 2026-10-16: Checks from the public site with an anonymous account of `bitneedle.com/id` against `https://mstr.bitneedle.com`. The authorizer accepts the `MSTR` token of an anonymous session: each request of a complete cloud job returned HTTP 200. After "Sign out", `/id/tokens` returns HTTP 401, but a token minted before the sign out still gave HTTP 200 298 s before its expiry, and HTTP 403 5 s after it. The `$default` stage limits every caller together to a burst of 10 and 5 requests per second; 20 `status` requests of one account in 1.81 s gave one HTTP 429. No limit applies to one account. Key rotation is not tested.

## D050: Music and Art routes are the app entry points

- Recorded: 2026-09-26.
- Origin: The user asked the splash to land on `/music`, removed `/app`, renamed the Tracks and Lightroom routes to Music and Art, and asked the expanded mobile tray to use desktop button geometry.
- Status: Accepted and deployed on `yl.vin`.
- Decision: The borderless splash wordmark enters `/music`. Music owns `/music/<track-id>/master` and the other audio-library details; Art owns `/art`; the record shelf remains `/mix`. App navigation uses those names and paths directly. `/app`, `/tracks`, and `/lightroom` return 404 instead of serving alias routes. The expanded mobile rail uses the desktop button width, 45-pixel height, spacing and padding; its collapsed targets remain 44 pixels.
- Reason: The visible section names and URLs now agree, and the public entry opens the source library where MASTER starts. Removing obsolete prototype paths avoids parallel route contracts. Equal open-tray geometry prevents the longer mobile buttons the user observed.
- Alternatives: Keeping `/app` and old aliases would preserve stale links but conflict with the explicit route removal and the app's prototype rule. Leaving the mobile tray at 260 pixels would keep controls visibly longer than their desktop counterparts.
- Evidence: Local and deployed Chromium/WebKit route tests open the splash link, select Music and Art, and receive 404 for all three removed paths. The navigation geometry test compares all five buttons at 320, 390 and 768 pixels with desktop. The splash test checks zero link border at mobile and desktop sizes. Local native/WASM MASTER parity still produces Essential hash `490645200a3b46f24ca4efdf383ef21e12fc79cbb6bd8eb41270bf71bb4cb254`; the deployed `/music` cloud flow produces the same hash through the Rust API and Lambdas. Account and Stripe return-route regressions pass.
- Limits: A signed fixture token exercised the deployed MASTER job; a real person's signed-in browser session is still untested. Obsolete URLs deliberately have no redirect.
- Verification: Browser route, layout, splash, local MASTER and live cloud MASTER checks pass. The identity and artist Workers and the site Worker were redeployed after route changes.
- Review conditions: Revisit navigation only after actual phone feedback or a demonstrated external deep-link requirement; keep route changes explicit rather than restoring silent aliases.
- Extends: [D049](#d049-rust-only-mstr-api-and-zeroth-aws-authorizer) without changing DSP, candidate selection, or MSTR token verification.
- Work reference: The app's web layout notes (`docs/WEB_APP_LAYOUT.md`) and MASTER contract (`docs/MASTERING_BETA.md`).

## D051: Art and music records use a single-column route

- Recorded: 2026-09-26.
- Origin: The user replaced `/mix` with `/artmusic`, asked for records below the navigation in one column, and removed the visible Lambda processing and Account link from MASTER.
- Status: Accepted and deployed on `yl.vin`.
- Decision: The record shelf, deck, record details, app links, and artist-page returns use `/artmusic`. `/mix` returns 404. The shelf's destination links precede its records in one narrow column; record cards stack one per row and the records-per-row control is removed. MASTER still follows the Account processing preference, but shows no mode banner or Account shortcut.
- Reason: The requested address names the combined Art and Music destination. A single sequence makes the destinations and records readable without a split desktop layout. The processing preference remains available in Account without repeating its setting on MASTER.
- Alternatives: Keeping `/mix` as an alias would retain the obsolete prototype path. Retaining the column count control would conflict with a fixed single-column shelf.
- Evidence: The previous release's `/mix` shelf used a 400px navigation column beside a separate record grid on wide screens. The visible MASTER header contained “LAMBDA PROCESSING” and “CHANGE IN ACCOUNT.”
- Validation: Local and deployed Chromium/WebKit checks opened `/artmusic` with navigation before the record shelf; direct HTTP checks confirmed `/mix` and `/app` return 404. The MASTER sign-in gate and mixed-rate rendering checks passed before the committed UI test scripts were removed. DSP, cloud rendering, and token verification are unchanged.
- Review conditions: Revisit the shelf width only after testing with a populated record library on desktop and mobile.
- Replaces the `/mix` route and split-shelf layout portions of [D050](#d050-music-and-art-routes-are-the-app-entry-points); its `/music`, `/art`, and mobile navigation decisions remain.

## D052: Let the Rust engine validate cross-rate stem alignment

- Recorded: 2026-09-26.
- Origin: MASTER rejected a saved four-stem set with “Stem estimates do not align with the source” before invoking Rust. The browser compared raw decoded frame counts and sample rates, while Rust already accepts aligned estimates at a supported rate different from the source.
- Status: Accepted; implemented in the browser bridge and validated headlessly.
- Decision: Remove the browser's exact-rate and frame-count check. Pass all four estimates to the same Rust set analyzer used by native jobs. Rust alone checks supported sample rates, resampled duration within one frame, four-stem consistency, finiteness, and reconstruction quality before using stem evidence. Rendering continues to use the original stereo mix.
- Reason: A 44.1 kHz source and 48 kHz stems can represent the same timeline. Comparing their raw frame counts rejected valid estimates and made the browser stricter than native.
- Alternatives: Resampling in JavaScript would duplicate engine policy; forcing one rate for stored stems would rewrite original estimates. Neither is needed.
- Evidence: The headless fixture with a stored 44.1 kHz source and four 48 kHz estimates completes MASTER. The delivered WASM master and native master have the same SHA-256 when native receives the same stored source PCM and stems. The ordinary 48 kHz native/WASM fixture still matches byte for byte.
- Limits: Importing the original 44.1 kHz WAV into SoundKit can change the PCM representation before mastering; comparing native from the pre-import WAV against WASM from the stored source is therefore a different-input comparison. The browser and native parity test now uses the actual stored source for this cross-rate case. The user's specific track and its existing estimates remain untested without its private audio.
- Validation: Local Chromium MASTER render, source preservation, version download, report, matched playback and native/WASM output hash checks pass for the mixed-rate and ordinary fixtures. The Rust engine's alignment and reconstruction tests remain the authority for unusable estimates.
- Review conditions: Revisit input import fidelity separately if byte parity with a pre-import 44.1 kHz file is required; do not reinstate a browser-only alignment rule.
- Extends: [D048](#d048-signed-in-lambda-master-with-explicit-local-processing) and [D051](#d051-art-and-music-records-use-a-single-column-route).

## D053: Search remains a quiet utility field

- Recorded: 2026-09-26.
- Origin: The user rejected a search treatment that looked like navigation and asked for a low-key, more neubrutalist type treatment instead of a Times-like font.
- Status: Accepted and deployed on `yl.vin`.
- Decision: App search inputs use sentence-case prompts, a small monospace face, transparent ground, square edges, and a thin muted underline. Focus darkens the underline. The stamped display face and heavy rule stay on navigation and actions.
- Reason: Search should be findable and readable without competing with the primary destinations or resembling another navigation item.
- Alternatives: A boxed input adds panel weight; an uppercase poster font and heavy rule repeat the navigation language.
- Evidence: Headless mobile screenshots of Music and Art/Music show the field in the top bar with a lighter visual weight than the destination links and add action. Computed style reports a 1px muted underline and monospace text.
- Validation: Local mobile and desktop screenshots were inspected. Live Chromium and WebKit reported the sentence-case placeholder, monospace face, thin underline, and navigation above records. The desktop topbar gained a one-pixel boundary to anchor its first row; mobile keeps its existing shell boundary.
- Review conditions: Revisit after real-device readability feedback, especially the mobile 16px anti-zoom size.
- Extends: [D051](#d051-art-and-music-records-use-a-single-column-route).

## D054: Remove committed UI automation

- Recorded: 2026-09-26.
- Origin: The user explicitly asked to delete all UI tests and add an AGENTS.md instruction against adding them again.
- Status: Accepted; implemented in bitneedle-app and both workspaces' AGENTS.md files.
- Decision: Remove committed browser UI scripts, UI-only JavaScript component tests, the iOS UI test target and sources, and the shell wrappers that invoked them. Do not add browser, screenshot, visual-regression, or iOS UI automation tests. Use disposable manual or headless checks for interface changes; retain Rust DSP, protocol, and security regression tests.
- Reason: The user does not want a maintained UI test suite. The product can still be checked during a change without committing UI harnesses.
- Alternatives: Keeping only positive UI tests or converting them to snapshots would still maintain the unwanted suite.
- Evidence: The browser UI script set and iOS `BitneedleUITests` target were removed; XcodeGen accepted the revised project. Live route, search, and MASTER checks ran before removal.
- Validation: `xcodegen generate -q` and repository searches found no committed browser UI script or iOS UI test target. Engine `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --release` passed.
- Review conditions: Keep non-UI regressions for engine and service rules; add UI automation only if the user explicitly reverses this decision.
- Extends: [D051](#d051-art-and-music-records-use-a-single-column-route) and [D053](#d053-search-remains-a-quiet-utility-field).

## D055: Align the first rules of desktop DETAILS and MASTERING

- Recorded: 2026-09-26.
- Origin: The user said the tops of desktop columns looked broken while the mobile layout had better structure.
- Status: Accepted and deployed on `yl.vin`.
- Decision: The desktop topbar has a hairline boundary. In the two-column DETAILS/MASTERING view, MASTER's first section does not inherit the global section top margin. Mobile retains its existing spacing and shell boundary.
- Reason: The inherited margin put the MASTER rule 26px below the DETAILS rule, making the adjacent column heads appear unrelated. The narrow layout shows one column at a time and benefits from its existing vertical space.
- Alternatives: Moving all section margins or restructuring the headings would disturb other views. The local override changes only the misaligned pair.
- Evidence: A disposable 1440px Chromium screenshot and geometry check put the DETAILS first rule at 192.0px and MASTER at 190.0px after the fix. The earlier screenshot showed the MASTER rule roughly 26px lower. The mobile screenshot retained its original spacing.
- Validation: Local screenshot and computed geometry passed. The deployed stylesheet contains the desktop override; the site Worker reports a successful deployment.
- Review conditions: Revisit if a real desktop view with a long title or artist byline reveals a new mismatch.
- Extends: [D051](#d051-art-and-music-records-use-a-single-column-route) and [D053](#d053-search-remains-a-quiet-utility-field).

## D056: Cloud previews and on-demand lossless delivery

- Recorded: 2026-09-26.
- Origin: The user proposed returning high-quality Opus for listening, retaining Lambda-generated PCM in S3, and sending a large WAV only when its download button is used.
- Status: Proposed; no codec, object-retention, or API change is implemented.
- Proposal: Preserve the existing source-bound Rust analysis, family renders, and duplicate finalizer. Keep candidate PCM private in S3 while finalizing; never offer rejected versions to the browser. Retain each accepted verified PCM24 WAV under an immutable owner-bound delivery key. Encode the accepted versions and original comparison at a provisional 256 kb/s stereo into indexed SoundKit-v2 Opus (`.sk2` plus `.skidx`) using the existing Rust SoundKit codec. The browser should request a preview only when selected, seek with byte ranges, and play it through one clock with matched gain. The WAV button should request a fresh short-lived signed GET through the `MSTR` control API and stream that one verified WAV to a local file. The original local source remains unchanged.
- Reason: The measured track-e job sent 536.63 MB of eight accepted WAVs to the browser. At 256 kb/s, eight 232.91-second Opus previews would be about 59.6 MB before container overhead, or about 7.5 MB if only one is selected. This changes client transfer and storage without changing mastering decisions or PCM24 delivery bytes. The family Lambda already uploads files with Rust `ByteStream::from_path`, so direct renderer-to-S3 multipart streaming is not required for this gain.
- Alternatives: Keep eager WAV downloads (simplest, high transfer); re-render PCM only when downloaded (extra latency and retained source/plan requirements); encode every candidate before finalization (wasted codec work on duplicates); use a new Ogg/WebM container (extra muxing and player compatibility work). A direct renderer-to-S3 stream would need multipart completion, abort, hash, and retry semantics and is a separate resource optimization.
- Evidence: The Lambda worker uploads delivered and raw candidate WAVs to the one-day private job bucket. `control.rs` currently returns presigned GETs for accepted WAVs, and `web/js/mastering.mjs` downloads every accepted file immediately. The full track-e run measured 536,632,384 browser-download bytes. AWS documents Rust file-backed `ByteStream`, S3 range GETs, multipart upload, and lifecycle abort of incomplete parts: [Rust S3 examples](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/rust_s3_code_examples.html), [partial object downloads](https://docs.aws.amazon.com/AmazonS3/latest/userguide/download-objects.html), [multipart uploads](https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpuoverview.html).
- Validation plan: Confirm exact accepted version IDs/names/PCM24 SHA-256 hashes are unchanged; measure preview encode time, size, peak and integrated-loudness drift; audition Opus against WAV on complex material; exercise on-demand seeking, shared position and matched gain in Chromium and WebKit with disposable headless sessions; measure first-preview latency, total transfer, Lambda CPU/memory, S3 reads and storage; verify owner-bound URL issuance, expiry/refresh, immutable uploads, failed range requests, and one-click WAV SHA-256 validation. Keep backend/codec regression tests, not committed UI tests.
- Open decision: Accepted-WAV retention and account quota. The present `jobs/` bucket expires everything after one day, which is incompatible with a lasting download button. For alpha, use a separate accepted-delivery prefix/bucket with an explicit retention period and display expiry if temporary; choose durable account storage only with quota/deletion policy. Reject candidate objects can keep their one-day lifecycle or be deleted after finalization.
- Review conditions: Approve retention and preview bitrate after listening/cost checks. Stream finalizer S3 reads to scratch files as a bounded-copy improvement; implement preview delivery first and revisit direct PCM multipart streaming only if measured `/tmp`, memory, or upload latency justifies it.
- Extends: [D048](#d048-signed-in-lambda-master-with-explicit-local-processing), [D049](#d049-rust-only-mstr-api-and-zeroth-aws-authorizer), and [D052](#d052-let-the-rust-engine-validate-cross-rate-stem-alignment).

## D057: Upload a FLAC source to cloud mastering

- Recorded: 2026-09-26.
- Origin: The user specified FLAC rather than uncompressed PCM for audio sent up to Lambda while discussing cloud previews and downloads.
- Status: Superseded by [D058](#d058-upload-the-existing-soundkit-source-and-keep-float-dsp); its native-FLAC transcode and float-source policy do not match the stored-source contract.
- Decision: The browser uploads one original stereo mix as a native `.flac` object to the private job bucket. The Lambdas create the master versions; the browser does not upload pre-rendered versions. Replace the current `original.wav`/`audio/wav` presigned PUT and worker input contract with `original.flac`/`audio/flac`. Keep the local library's original file unchanged. Decode the *same uploaded FLAC bytes* for source-bound planning in the browser and for rendering in Lambda, then enforce the existing source-frame and PCM-fingerprint checks. Apply the same canonical decode before local-only mastering so local and cloud runs receive the same samples. Reject corrupt, unsupported, or mismatched FLAC instead of silently falling back to a different source.
- Reason: The current browser sends an uncompressed float32 WAV even though SoundKit already includes a Rust native-FLAC encoder and decoder in WASM. FLAC reduces upload bytes for compressible sources without changing the integer PCM it encodes. A single canonical decode boundary preserves the source binding and makes the native/WASM comparison meaningful.
- Alternatives: Upload the existing `lossless.sk2` when present (its frames contain FLAC, but it is a SoundKit container and is not always present); upload the original imported file (may be lossy or unsupported by Lambda); keep float32 WAV (large upload); encode master candidates in the browser (duplicates Lambda work and weakens the source-bound plan).
- Limits and counterexamples: [FLAC stores integer PCM and does not support floating-point samples](https://xiph.org/flac/faq.html). Existing 16/24-bit lossless sources can be carried without sample change if their SoundKit import preserved the integer samples. A source available only as decoded Opus or float32 must be converted to an explicit integer depth before FLAC encoding; that is a change to its decoded samples, not lossless preservation of the original float stream. Do not claim original-file bit identity for those cases. Evaluate 24- versus 32-bit FLAC size, encode time, and measured round-trip error before choosing the canonical depth. The original imported file remains stored untouched.
- Evidence: `bitneedle-app/web/js/mastering.mjs` currently uploads `masterFile(..., 'original')` as `audio/wav`; its worker stages a decoded source and writes an original WAV into `audio.bin`. The control Lambda signs and validates `original.wav`; `main.rs` reads it as WAV; `src/distributed.rs` compares the decoded source fingerprint with the plan. SoundKit `WasmFlacEncoder` emits native FLAC from integer-quantized planar float samples, and `soundkit-flac` has a native stream decoder. This is a design update, not a measured FLAC benchmark.
- Validation plan: Encode one canonical source per job, decode that exact blob in browser and Lambda, and compare frame count, sample rate, source PCM SHA-256, plan, accepted IDs, and delivered WAV hashes against a local run on the same decoded FLAC. Test 16/24-bit lossless imports, Opus-only imports, and float32 imports; reject truncated/bad FLAC and object substitution. Measure compressed bytes, browser encode time, upload time, Lambda decode time, and total wall time on track-e and at least one complex track. Keep these as codec/protocol regressions, not UI tests.
- Review conditions: Set the integer-depth policy from the round-trip and cost measurements; revise only if it meaningfully changes mastering analysis or proves too expensive on mobile. The Opus-preview and on-demand-WAV proposal in [D056](#d056-cloud-previews-and-on-demand-lossless-delivery) remains separate from source upload.
- Extends: [D056](#d056-cloud-previews-and-on-demand-lossless-delivery) and [D048](#d048-signed-in-lambda-master-with-explicit-local-processing).

## D058: Upload the existing SoundKit source and keep float DSP

- Recorded: 2026-09-26.
- Origin: The user clarified that lossless imports already have a FLAC version, lossy imports have an Opus version, and asked whether the mastering engine could process signed 24-bit integers more efficiently than floats.
- Status: Accepted and deployed for source transport; retained DSP design. Synthetic FLAC and Opus jobs are validated, while full-track transfer and listening measurements remain open. Replaces [D057](#d057-upload-a-flac-source-to-cloud-mastering).
- Decision: Select the same stored source that local MASTER already selects: `lossless.sk2` (SoundKit-v2-framed 24-bit FLAC) if present, otherwise `stream.sk2` (SoundKit-v2-framed Opus). Upload that existing immutable blob once per cloud job, with an explicit format/codec declaration and an owner-bound create-only key. Do not decode to WAV or re-encode to native FLAC for transport. Make the native Lambda decode the uploaded SoundKit stream with the same Rust codec semantics as WASM, resample to 48 kHz, and enforce the existing decoded-PCM fingerprint and source-frame binding before rendering. Keep the current `Stereo = [f32; 2]` DSP representation, with `f64` where the engine already uses it for filter states, coefficients, detector/measurement accumulation, and resampling. Retain PCM24 quantization and dither at final delivery only.
- Reason: The local mastering worker already decodes the selected SoundKit stream to float PCM. Uploading its existing bytes avoids the float WAV currently created for cloud transport and avoids an unnecessary second FLAC encode. Signed 24-bit source samples scaled by a power of two fit exactly in `f32`; float processing provides intermediate headroom for gain, filtering, oversampling and limiting without clipping or quantizing after every stage. `f32` and 32-bit fixed-point both occupy four bytes per sample. Packed signed 24-bit would reduce raw scratch bytes by 25% but require unpacking and repeated fixed-point scaling/rounding; it has no demonstrated runtime advantage and would change the engine's current exact-output contract.
- Alternatives: Native `.flac` transcode for every source (adds work and cannot losslessly represent decoded Opus or arbitrary float samples); float32 WAV upload (current, larger); signed-24 fixed-point DSP (needs explicit headroom, wider multiplication/accumulators, saturation and new parity/listening validation); 32-bit fixed-point DSP (same sample-memory footprint as `f32` and still needs headroom policy). Compressing temporary float scratch separately may be worth measuring if scratch I/O becomes a bottleneck; it is independent of the source-upload codec.
- Evidence: `bitneedle-app/web/js/audio-export.mjs` chooses `lossless.sk2` or `stream.sk2`; its import worker writes the FLAC copy for sources classified lossless. `web/js/workers/mastering.worker.mjs` calls `WasmSoundKitV2Decoder.pushFloat`, then stages `f32le`. SoundKit's v2 FLAC float decoder scales signed samples by `2^(bits-1)`; its lossless library copy is 24-bit. `src/audio_io.rs` defines `Stereo = [f32; 2]`; `src/resample.rs`, `src/dsp.rs`, `src/render_blocks.rs`, and `src/scratch.rs` show float arithmetic and storage; `src/delivery.rs` quantizes final PCM. The present cloud control/family/finalizer contract still expects `original.wav`. The 25% packed-scratch saving follows from 3 versus 4 bytes per sample; no integer-DSP speed or quality benchmark has been run.
- Validation plan: For each codec, upload the stored `.sk2` bytes unchanged, compare their SHA-256 before and after S3, and compare native versus WASM decoded 48 kHz PCM fingerprints, plans, accepted IDs, and PCM24 WAV hashes. Test lossless 16/24-bit inputs, Opus-only imports, cross-rate inputs, truncated streams, and substituted objects. Measure source upload bytes/time and Lambda decode time against the current WAV path. Consider an integer or compressed-scratch spike only after profiling shows a material DSP or scratch bottleneck; require quality, parity, and throughput measurements before changing sample representation.
- Limits: The stored lossless copy is 24-bit, even when an imported file had another representation; this decision preserves the existing stored source, not necessarily the external file's original bytes. Opus is lossy by design, but the cloud job must receive the *same Opus bytes* that local MASTER analyzes. Intermediate float values can exceed 0 dBFS; signed-24 DSP would need an explicit wider internal format to avoid clipping. FLAC itself stores integer PCM, not floats ([Xiph FAQ](https://xiph.org/flac/faq.html)).
- Review conditions: Revisit `f32` only with measured evidence of a bottleneck or audible/numerical error. Keep source transport, internal DSP precision, and final output depth as separate decisions.
- Extends: [D056](#d056-cloud-previews-and-on-demand-lossless-delivery), [D048](#d048-signed-in-lambda-master-with-explicit-local-processing), and [D052](#d052-let-the-rust-engine-validate-cross-rate-stem-alignment).

- 2026-09-26 implementation update: The browser now uploads the selected `.sk2` blob instead of its float32 comparison WAV. The Rust `MSTR` control Lambda binds `lossless` or `opus` to an immutable source key and signs a create-only PUT. Family and finalizer Lambdas use the pinned SoundKit v2 Rust decoder, reject a codec/key mismatch and truncated stream, then retain the existing resampled-PCM fingerprint check. SoundKit revision `15c2cc0ff08619e73b37c5290387c370cc4f9ea0` fixes the minimal FLAC/Opus feature build for the native Lambda. Codec/control tests and Clippy passed; live cloud validation and transfer measurement are pending.
- 2026-09-26 deployment and validation update: The US East 1 control, family, and finalizer Lambdas and `yl.vin` browser asset are deployed. Synthetic six-second SoundKit FLAC and Opus sources completed owner-bound create-only upload, plan submit, family render, duplicate finalization, and signed WAV download. S3 source bytes matched the uploaded SHA-256; headless Chromium and native decoded PCM SHA-256 matched; both accepted PCM24 WAV SHA-256 hashes for each codec matched local native `render-family`. The unsigned API remains HTTP 401. A real signed-in browser session and full-track upload are still unmeasured.

## D059: Review the stored Opus quality for future imports

- Recorded: 2026-09-26.
- Origin: The user asked whether the current 192 kb/s Opus library copy should be raised to “full quality” while enabling compressed source uploads.
- Status: Proposed; the 192 kb/s import profile remains implemented.
- Proposal: Upload existing Opus objects unchanged for mastering. Do not re-encode an existing 192 kb/s object to a higher rate. For future imports, compare the current 192 kb/s SoundKit profile with 256 and 320 kb/s on demanding material through level-matched blind listening and encoded-size/CPU measurements; change the single library import profile only if a higher rate audibly improves decisions or playback. Lossless imports continue to use their FLAC copy for mastering regardless of the Opus playback copy's rate.
- Reason: Opus has no “full quality” lossless setting. The [Opus tools documentation](https://opus-codec.org/docs/opus-tools/opusenc.html) calls 160 kb/s a very-high-quality example and allows higher target rates; the [Opus specification](https://datatracker.ietf.org/doc/rfc6716/) says higher bitrate generally improves quality but places the full-band stereo music sweet spot below the current 192 kb/s. A rate increase spends storage and upload bytes without restoring information lost in an already encoded source. SoundKit's current import feeds 16-bit PCM into the Opus encoder, so changing rate alone is not a high-resolution input-path change.
- Alternatives: Raise all future imports directly to 256/320 kb/s without listening evidence; transcode old 192 kb/s streams upward (no information gain); retain original imported lossy files alongside SoundKit streams (separate storage and source-contract decision).
- Evidence: `soundkit-wasm/src/lib.rs` sets the streaming library's Opus bitrate to `192_000` in both import constructors. `soundkit-stream/src/lib.rs` encodes the stream from interleaved `i16` samples. For a 232.91-second stereo track, nominal 192, 256 and 320 kb/s payloads are about 5.6, 7.5 and 9.3 MB before SoundKit framing; actual sizes and perceptual differences remain unmeasured here.
- Validation and review conditions: Compare complex, bright, transient-rich and quiet material with the same SoundKit encoder/decoder in blind level-matched trials. Measure import encode time, library storage, cloud source upload and mastering-plan stability. Adopt a higher rate only if its benefit justifies the cost; do not describe any Opus rate as lossless.
- Extends: [D058](#d058-upload-the-existing-soundkit-source-and-keep-float-dsp).

- 2026-09-26 scope update: [D060](#d060-preserve-decoded-source-in-flac-for-every-new-import) makes Opus a playback copy for new imports. This proposal now concerns playback quality; it does not govern the mastering source. Existing Opus-only imports still use their original stored stream until reimported.

- 2026-09-26 replacement update: [D061](#d061-require-flac-for-mastering-and-source-delivery) removes the historical Opus-only mastering path entirely. Opus bitrate remains a playback-only question.

## D060: Preserve decoded source in FLAC for every new import

- Recorded: 2026-09-26.
- Origin: The user required that lossy AAC/MP3 uploads avoid an additional lossy generation before mastering, while the LOSSLESS badge describe only a genuinely lossless supplied source.
- Status: Accepted and implemented in the browser importer; deployment and real-track validation remain to be recorded.
- Decision: SoundKit writes `lossless.sk2` FLAC from decoded PCM for every new import, plus the existing 192 kb/s Opus playback copy. Both local and cloud MASTER use the FLAC copy when present. SoundKit exposes the detected source codec's lossless status independently of FLAC creation; the library records it in the track note and restores it from there. Older Opus-only tracks keep their exact stored source until the original file is reimported. Raise the cloud SoundKit source cap from 125 MB to 200 MB, covering the supported five-minute 96 kHz stereo 24-bit geometry.
- Reason: Transcoding AAC or MP3 to Opus loses another generation; storing decoded source as FLAC avoids that encode. FLAC cannot recover detail already lost in the supplied file, and the presence of a FLAC copy does not prove lossless provenance.
- Alternatives: Raise Opus bitrate (still lossy); retain every original compressed file and add AAC/MP3 decoding to cloud Lambdas (smaller upload but a broader source contract); transcode existing Opus tracks to FLAC (preserves the already-degraded signal but cannot recover the uploaded source).
- Evidence: `web/js/workers/import.worker.mjs` previously passed a filename-based lossless flag into `WasmLibraryImport`, so AAC/MP3 made no FLAC. `web/js/library.mjs` previously restored the badge from `hasFile("lossless.sk2")`; this would mislabel all lossy imports once FLAC is universal. SoundKit's preservation encoder stores 24-bit PCM at source geometry, while Opus storage is 16-bit-derived 48 kHz stereo.
- Validation: Require an AAC/MP3 import to produce nonempty FLAC and Opus streams; verify source badge before and after reload, FLAC selection for MASTER, cloud codec binding, and engine release checks. Listen and measure the 24-bit quantization boundary separately; no listening claim is made here.
- Review conditions: Revisit storage quota, import time, 24-bit integer clipping of above-full-scale decoder output, and exact PCM equivalence to the lossy decoder on a real track. Do not label a lossy source LOSSLESS even though its decoded copy is stored in FLAC.
- Extends: [D058](#d058-upload-the-existing-soundkit-source-and-keep-float-dsp); narrows [D059](#d059-review-the-stored-opus-quality-for-future-imports) to playback.

- 2026-09-26 validation update: Headless Chromium imported an MP3, AAC/M4A, and WAV through the deployed `yl.vin` SoundKit WASM importer. Each yielded nonempty, decodable FLAC and Opus streams; `sourceAudio` selected FLAC. After a library reload, MP3 and AAC had no LOSSLESS badge and WAV did. The MP3 remained unbadged even when named `.wav`, proving the new badge follows codec detection. The stored FLAC bytes were 103,790 / 541,217 / 106,388 respectively for these short fixtures. SoundKit source-provenance getter revision `350ce8d` is pushed. Engine and Lambda format/Clippy/tests pass. Real-track listening, quota measurements, and signed-in cloud upload remain unmeasured.

- 2026-09-26 replacement update: [D061](#d061-require-flac-for-mastering-and-source-delivery) removes the remaining Opus fallback and any source-provenance guess from older notes. The FLAC-on-import and recorded codec-provenance parts of D060 stand; its fallback provision is superseded.

## D061: Require FLAC for mastering and source delivery

- Recorded: 2026-09-26.
- Origin: The user clarified that the app always saves FLAC and that the no-legacy-fallback rule in the workspace agent instructions applies to this prototype contract.
- Status: Accepted, implemented, and deployed in US East 1 and on `yl.vin`.
- Decision: Keep Opus as a separate playback copy, but require SoundKit FLAC for MASTER analysis, local render, cloud upload, source export, and TAPE publication. The `MSTR` create request has no source-codec choice; it signs one immutable FLAC key. Rust family and finalizer Lambdas reject any non-FLAC key or SoundKit frame. Older tracks lacking FLAC stop with an instruction to reimport the original file; no Opus transcoding or fallback path remains. Older track notes without explicit codec provenance receive no LOSSLESS badge. Playback reads its Opus copy directly.
- Reason: A lossy-to-Opus generation must never silently become the input to mastering. One source contract keeps client, plan, upload, and native render aligned. Reimporting the original is the only way to recover the decoded signal before the obsolete Opus copy was made.
- Alternatives: Keep the historical Opus fallback for older tracks (rejected as an obsolete prototype contract); transcode the stored Opus to FLAC (cannot recover the input and conceals generation loss); retain original AAC/MP3 and decode them on Lambda (broader cloud codec contract than needed).
- Evidence: The workspace agent instructions say to replace obsolete prototype contracts without legacy fallbacks unless requested. SoundKit WASM now writes FLAC for every new import and a deployed headless MP3/AAC/WAV check selected it. The earlier synthetic cloud run verified native/WASM FLAC PCM and accepted-WAV parity; its Opus branch is historical only.
- Validation: Require FLAC source selection and a clear reimport error for older tracks, reject `source_kind` create requests and Opus source objects, and rerun Lambda unit tests, Clippy, engine release tests, a cloud FLAC render, and live browser import checks. Browser signed-in full-track flow remains to be measured.
- Review conditions: Reconsider only if a new explicit source-provenance and codec contract is accepted; never silently replace decoded source PCM with a lossy intermediary.
- Replaces: Opus fallback portions of [D058](#d058-upload-the-existing-soundkit-source-and-keep-float-dsp) and [D060](#d060-preserve-decoded-source-in-flac-for-every-new-import).

- 2026-09-26 deployment and validation update: The FLAC-only control/family/finalizer stack is deployed in US East 1 with image digest `sha256:795eb3e20e03be7200ff6f247cbb5a0a4b98eb993bb5834d48c0454e9b5f3f71`; the matching site release is deployed. Direct invocation of the deployed control Lambda rejected an obsolete `source_kind: opus` create request with HTTP 400. A new FLAC job uploaded through its signed URL, finished both families and finalization, and downloaded WAVs whose hashes matched the earlier native results (`ee2ce638…` and `296a69fc…`). Unsigned API Gateway POST remains HTTP 401. Headless Chromium against deployed `yl.vin` imported MP3, AAC/M4A, and WAV through SoundKit WASM and selected FLAC for each; MP3/AAC kept no LOSSLESS badge, WAV retained one, and an Opus-only stored track produced the explicit reimport error. Lambda format, Clippy, and unit tests and engine release checks pass. Full signed-in browser validation, listening, and quota measurements remain open.

## D062: Diagnose full-range, all-stem and passage coverage before judging version counts

- Recorded: 2026-09-26.
- Origin: The user challenged track-f's two-version result versus track-e's eight,
  described track-f as more dynamic and varied, requested master/stem analysis, and
  reiterated that the intended engine inspects all frequency bins/bands and stems.
- Status: Accepted scope clarification and completed diagnostic audit. Broader
  candidate generation and treatment changes remain unimplemented proposals.
- Decision: Treat the current vocal-overlap family as partial coverage. Evaluate
  full-range spectral, temporal and spatial evidence across every supplied stem
  and the original mix. Preserve local opportunities and measure collateral effects
  before whole-programme aggregation. All rendered masters still use original stereo.
- Reason: The 29-band evidence measures all four roles, but automatic proposals use
  vocal versus summed accompaniment, 200–4000 Hz, and whole-song medians. Passage
  refinement runs after that gate. Density and limiter timing do not supply the
  omitted all-stem tonal/dynamic search.
- Evidence: The track-f/track-e analysis. The source LRA of track-f is
  2.4 LU versus 0.8 LU; mix rise counts are 391 versus 12 under the same detector.
  Local mid/side opportunities occur despite rejected global medians. Actual bell
  probes include both positive passage responses and contrary cross-stem effects.
  Fresh track-f masters reduce independent LRA to 1.0/0.6 LU. These observations do
  not prove preference, audibility, separation purity or a required number of masters.
- Alternatives: Infer completeness from matching Python/Rust proposals, lower
  thresholds to obtain eight, or add fixed presets. None addresses the missing
  role/time coverage or establishes a measured treatment benefit.
- Proposed implementation: Carry role-specific full-range/time evidence through
  generation, probe realizable treatments and collateral responses, then compare
  full renders. Investigate gentler transient/dynamics handling and passage focus
  on track-f. Keep unchanged processing and existing liked outputs as controls.
- Verification: Hash-bound source/stem analysis, full-bin evidence, replayed auditory
  medians, native track-f set, independent delivered meters; formatting, strict Clippy
  and release tests pass. Diagnostic filters have not been promoted or auditioned
  as new complete masters. No production rule or DSP change is made by this entry.
- Review conditions: Require positive and contrary fixtures for alternating spatial
  directions, minority passages, non-vocal regions and unchanged cases; validate
  complete-chain output and listening before treating revised heuristics as supported.
- Work reference: TODO, "Listening and broader treatment work".
- Clarifies: D034, D038, D040 and D045. Historical parity results remain valid;
  they establish implementation agreement, not completeness of candidate coverage.

## D063: Reassess assumptions across the confirmation mixes and the premixes

- Recorded: 2026-09-26.
- Origin: The user requests a page-one review of assumptions by processing and
  mastering all confirmation mixes and premixes through the new tool chain, and
  permits reliance on the masters of an online mastering service only
  for loudness, explicitly excluding dynamics.
- Status: Accepted evaluation scope; complete corpus run and technical verification.
  Musical calibration and proposed production-rule changes remain open.
- Decision: Freeze the current original-mix Rust set renderer as a controlled
  baseline. Reassess its assumptions using all distinct source revisions, all
  validated stems, full-range spectra and local temporal/spatial evidence. Generate
  verified full-song masters and expose them for listening before claiming quality.
- Reference policy: Use the integrated loudness of the online service masters only. Do not adopt its
  dynamics, transient behaviour, compression, tone, width or sound as desired targets.
  Record revision pairing; different mixes cannot supply exact same-source claims.
- Controlled extension: Render each source's Essential spec at the same-song
  scalar level of the online service master, keeping its source-derived recovery and peak ceiling.
  Record exact filename versus contextual revision pairing. These are evaluation
  outputs, not automatically approved production alternatives. This isolates the
  effect of the target choice within our chain; listening still determines usefulness.
- Listening interface: A local corpus page includes the original, every automatic
  version and the level comparison, with position-preserving switching and matched
  or delivered playback. Show source-relative measurements and pairing context.
- Reason: The track-f case showed that global vocal-overlap proposals can miss local and
  other-stem evidence. Current-set output count and Python/Rust parity do not
  establish completeness. A common corpus/binary makes the next review concrete.
- Alternatives: Tune track-f alone; compare new outputs with unrelated historical
  executables; copy the dynamics of the online service; infer that premixes differ only in processing.
  These would confound source, implementation or preference with the tested claim.
- Evidence: Corpus protocol and findings, starting with
  19 distinct recordings totalling 62.09 minutes and eight level-only references.
- Completed evidence, 2026-09-26: 94 automatic masters and 19 level comparisons,
  all delivered and independently verified. Essential reduces source LRA on all
  19 recordings (median 1.6 LU); level comparisons reduce attack-loss P90 on 16/19.
  Local evidence missed by global medians appears in drums/bass/accompaniment/vocals
  on 9/4/18/17 recordings respectively under the disclosed exploratory grid and
  gates. Retain the three contrary attack comparisons and track-e's LRA counterexample.
  These findings challenge coverage and preservation assumptions; they do not
  establish preferred processing, optimal recovery or a count of good treatments.
- Validation: Preserve source hashes, verify stem reconstruction and PCM24 output,
  meter independently, retain counterexamples and explicit uncertainty; run the
  required Rust gates and known-signal checks for added diagnostic calculations.
- Validation completed: All source identities and 113 deliveries pass. Formatting,
  Clippy, 156 release tests and four known-signal checks pass. Disposable desktop/
  mobile listening-page checks pass. One automatic Denser search stops 0.144 LU
  below target at its evaluation bound; all level comparisons are within 0.030 LU.
- Review conditions: Revise rules only with measured basis, regression coverage,
  full-render checks and listening. Keep unchanged processing eligible. No forced
  version quota or reference-driven dynamics objective is accepted.
- Work reference: TODO, "Corpus reassessment".
- Supersedes: D032's permission to use the online service's tone, width and dynamics as new
  reference evidence. Historical observations remain as history, not active guidance.
- 2026-09-27 interpretation update: The level comparison is diagnostic, not a
  quieter production proposal. Essential is 2.9–5.5 LU louder than its own source
  on all 19 recordings (median 4.4 LU), and its median advantage over the
  level comparison at the level of the online service master is 0.6 LU. Lower attack loss at lower level on 16/19
  establishes a level/dynamics tradeoff, not a preferred master. Retain the current
  louder versions as controls and test preservation changes at fixed delivered
  LUFS and peak ceiling before considering any rule change. The 11 contextual
  reference pairings remain unsuitable for exact revision-specific loudness claims.
  The two-versus-eight track-f/track-e count arises from zero versus three supported
  vocal-focus branches, each rendered at two densities; it does not rank their
  musical complexity. Two-second focus envelopes are reached only after a
  whole-song vocal/accompaniment gate. Bass and drum measurements inform recovery,
  but no local all-role/full-range proposal stage exists in the native automatic
  set. The track-f mix is one diagnostic case, not a special-case tuning target or a version
  quota. Extend D062's proposed coverage with one source-independent rule, continuous
  original-mix rendering, whole-song transition and collateral checks at fixed
  delivered level, and listening on the corpus plus unfamiliar contrasting sources
  before acceptance. See the dated interpretation in
  the corpus reassessment.
- Extends: D062's full-range, all-stem coverage requirement.

## D064: Evaluate all-role full-range local treatment trials

- Recorded: 2026-09-27.
- Origin: The user asks whether the automatic set overlooks relevant bass, drum,
  accompaniment or high-frequency bands, whether local changes are needed, and
  requests a new run of all mixes. The reference engineer's master is available
  only for track-e. The sound of the online mastering service is not a quality
  reference.
- Status: Accepted evaluation study, completed with an independent audit.
  No new automatic product rule is accepted or connected to `set::render`;
  the existing 94-version corpus remains the baseline.
- Decision: Reuse the hash-verified four-stem/full-range arrays for all 19 sources.
  In each role, find at most one adjacent-band local opportunity with at least ten
  reliable seconds, a measured +3 dB mid/side probe response of at least 0.25 dB,
  and the documented activity/role-share gates. Compare source-derived mid/side
  +1.5 dB bell treatments in complete Rust renders. The contribution follows
  smoothed two-second opportunity duty; filter and limiter state run continuously
  on the original stereo mix. Keep Essential's requested LUFS, limiter settings
  and −1 dBTP ceiling, and write new immutable study files.
- Reason: This tests whether the current whole-song vocal, 200–4000 Hz proposal
  gate excludes useful candidate directions while holding delivery level fixed.
  Renders across the whole corpus expose no-opportunity cases and unrelated
  roles; track-f is a diagnostic case, not a special treatment target.
- Alternatives: Treat every measured high-energy band, impose a fixed EQ curve,
  master windows independently, force a number of versions, or copy the sound of the
  online mastering service. None follows from the existing measurements or preserves programme
  continuity. Static whole-song versions of selected trial filters remain a
  separate comparison needed to judge whether local automation helps.
- Limits: An improved role/complement ratio is not a mastering quality score.
  Estimated stem labels may be wrong despite good reconstruction; +3 dB power
  probes approximate rather than exactly predict the +1.5 dB rendered bell.
  Current study selection lives in a deterministic offline script; it is not a
  production engine rule. Listening at matched and delivered level, especially
  through transitions, is required before any promotion. The frozen first run
  padded each local envelope with dry values beyond measured windows. This has
  no audio effect where the final measured duty is zero; one track-b trial
  eases a 0.023 final duty to zero. Record that case separately in the results.
- 2026-09-27 evaluation update: The frozen outside-range pass selected five
  candidates; three centres are 199.9999987 Hz because the saved float32 grid
  represents the 200 Hz boundary just below 200. Count those as boundary cases,
  not evidence of sub-200 Hz opportunity. The checked-in selector now excludes
  that rounded boundary, with a regression test. The frozen renders remain
  unchanged; two genuine >4 kHz candidates remain in the comparison.
- Evidence and validation: Study results record
  exact renderer/plan identities, 56 local candidates, five outside-pass trials,
  eight static controls and seven bass-body trials. All 76 independent delivery
  checks pass. Fifty of 56 local trials are near duplicates by the provisional
  −40 dB screen; median local residual is −43.6 dB versus −27.0 dB for static
  controls. Bass-body trials change audio but relieve mean limiting by a median
  0.015 dB, with a contrary track-f case. These findings reject direct promotion
  of the present local and bass-headroom trial rules. Regression checks
  cover the selection gate, envelope geometry, rounded 200 Hz boundary and
  delivery-window diagnostic. The previous corpus outputs and originals remain
  unchanged.
- Review conditions: Require a useful sound difference without unacceptable
  level, dynamics, tone or spatial discontinuity across diverse recordings;
  include valid no-opportunity, near-duplicate and contrary cases. Keeping an
  already good Essential or Denser master is a successful result; no version
  quota or forced treatment follows from broader measurements. Implement any promoted rule
  in Rust with complete-chain regression tests and measured reasons for each
  decision. No candidate is selected for production from numerical score alone.
- Extends: D062's coverage diagnosis and D063's frozen corpus assessment.

## D065: Integrate verified core treatment instead of role trial versions

- Recorded: 2026-09-27.
- Origin: The user identifies bass, vocal, drum and accompaniment decisions as
  fundamental mastering, wants Essential and Denser to be coherent complete
  masters, and asks for strong candidates rather than separate micro-adjustments.
- Status: Proposed design. The live `set::render` contract has not changed.
- Proposal: Use original-mix and validated all-role/full-range evidence to form
  at most one combined foundational treatment. Apply that same selected
  foundation to every recommended family, including Essential, Denser and any
  later character branch, then render and compare the complete chain at each
  family's delivered loudness. Retain the unchanged tonal path as a candidate
  during selection. Re-evaluate the foundation through every family's limiter
  and preservation checks. Create an additional character only when it has a
  distinct, measured complete-song effect and its source-specific basis supports
  that direction. Do not expose one version for each analysed role or band.
  Local treatment must cover sustained passages with continuous filter state;
  scattered positive frames alone cannot justify automation.
- Reason: The frozen local trial selector found 56 technically valid candidates.
  Over the actual song windows, their median mean two-second wet duty is 0.121,
  median P90 duty is 0.301, and median duration at duty ≥0.1 is 93 seconds.
  These replace earlier incorrect statistics calculated over padded dry windows
  beyond the end of the song. The liked vocal-side trial of the track-f confirmation
  mix has a −62.8 dB
  loudness-matched residual from Essential and a largest two-second level change
  of about 0.0012 dB. These are diagnostic probes, not strong product versions.
  Existing track-e focus versions show materially larger residuals (roughly
  −25 to −30 dB), while preference and safety are not established by residual.
- Alternatives: Promote all single-role trials, make all filters permanently wet,
  use stem prominence as a quality score, force a fixed version count, or lower
  loudness to recover dynamics. Each would confound one measured cue with the
  sound of the complete master. Keep explicit character choices distinct from
  source correction and density.
- Evidence and validation: The all-role local audit
  confirms that local versus static controls have sharply different effect
  sizes at fixed delivered level. The initial wet-duty summary counted padding
  past the song end; corrected song-only statistics are 0.121 median mean duty,
  0.301 median P90 duty and 93 median seconds at duty ≥0.1. Next measure
  combined multi-band candidates against Essential for tonal, temporal,
  spatial and limiter effects. Calibrate
  any materiality threshold with matched-level
  listening across this corpus and unfamiliar contrasting sources. Record
  counterexamples and retain no-treatment outcomes. Regression tests must cover
  planning, complete renders and duplicate/rejection behavior before activation.
- 2026-09-27 integrated pilot update: A shared, source-bound all-role local plan
  was rendered through Essential and Denser for all 19 corpus tracks using the
  frozen renderer. The first one-octave overlap limit excluded the user's liked
  vocal-side region of the track-f confirmation mix and was replaced in a new immutable v2 run
  by a half-octave limit; a regression test covers both overlap and shared-family
  application. All 38 v2 deliveries verify, but 20 are below the provisional
  −40 dB near-duplicate screen, with median matched residual −40.7 dB and median
  largest two-second level movement 0.040 dB. The track-f and track-e integrated versions
  remain near duplicates under that numerical screen. This establishes effect
  size, not listener preference; no production promotion follows from the
  measurements alone.
- 2026-09-27 listening update: The user reports preferring Integrated in both
  comparisons they tried. The track identities and playback-level mode have
  been requested and are not yet recorded. This is positive preference evidence
  for complete combined masters, including a counterexample to treating the
  −40 dB residual screen as an automatic rejection. Keep the screen as a review
  aid until matched-level listening calibrates it. Integrated and same-density
  baselines differ by at most 0.035 LU across the pilot, so a large delivery
  loudness advantage does not explain this feedback. If a foundation is selected
  in the future, make it part of each recommended family's base plan; the
  current duplicate comparison must not discard it solely for a small residual
  from the unchanged-tone path. Preserve the same shared foundation across
  recommended families; do not infer a universal EQ rule from two comparisons.
- Review conditions: Promote only a source-independent rule whose chosen complete
  masters are preferred in listening and have no material preservation or
  transition failures. A good unchanged Essential remains a successful result.
- Extends: D064's evaluation; proposes replacing the current per-region product
  output policy after validation, without erasing the historical D039/D040 runs.

## D066: Put conditional all-role local EQ in the shared set core

- Recorded: 2026-09-27.
- Origin: The user wants the fundamental per-band, passage-aware treatment to apply
  to every recommended master family, and asks to get the generalized engine working.
- Status: Provisional, implemented in the shared Rust stem analyzer and set planner.
- Decision: Measure mid and side powers for drums, bass, other and vocals across
  the 29 one-third-octave bands. Select at most one supported adjacent-band
  opportunity per role, reject same-domain centres within half an octave, and
  apply +1.5 dB bells with smoothed two-second local wet envelopes to the original
  stereo mix. Source/stem alignment and local reconstruction bound the decision.
  Share the selected foundation through Essential, Denser and every focus family.
  Render an unchanged-tone fallback for measured comparison without publishing
  it as an extra version, and allow a valid no-treatment result.
- Reason: Whole-song vocal-only gates omit locally justified bass, drum and other
  regions. The 19-track pilot delivered 38 technically valid combined masters,
  and the user preferred the two Integrated comparisons reported so far. The
  shared treatment is a core processing stage rather than a separate one-band
  product character.
- Alternatives: Permanently wet EQ risks treating passages without the measured
  cue; independent window renders break DSP continuity. Separate band/role
  versions create small, difficult-to-compare variants. Always applying EQ would
  contradict already-good sources. The bounded wet envelope uses continuous
  filter state and can remain at zero.
- Evidence and rule: D064/D065 and the integrated pilot.
  `src/foundation.rs` records role, domain, centre, duration, predicted probe
  effect, duty and gain. Its stem-power denominator sums other-role powers, an
  approximation that can differ from coherent complement power; it must be
  checked in listening. The old −40 dB duplicate screen remains provisional;
  processing order keeps the foundation as the Essential anchor when selected.
- Limits: This is conditional, offline two-second EQ automation per selected
  band, not a fast detector-triggered dynamic EQ or a guarantee that every band
  moves. The set planner's `dynamic_after_eq` remains empty. The prototype's
  +3 dB probe and +1.5 dB render gain,
  activity/share thresholds, overlap rule and two-second window are engineering
  choices, not proven universal settings. The existing vocal-focus families
  remain; their character and version count still need listening review.
- Verification: Unit/regression tests check a non-vocal bass proposal,
  independent bass/vocal band passage envelopes, short and unreliable no-op
  cases, shared-family composition and unchanged fallback. `cargo fmt --check`,
  strict all-target Clippy, `cargo test --release`, WASM target check and
  `wasm-pack build --target web --release` pass. The native
  rerender of track-f selected three local regions and completed two treated masters;
  its unchanged-tone trials were near duplicates of the corresponding treated
  densities. The track-e set retained all eight previous family identities with the
  foundation in their plans; its unchanged-tone trials were near duplicates.
  The native rerender records hashes, delivery
  checks, matched effects and source diagnostics. The remaining real-track
  rerenders are recorded with the run result.
  Listening beyond the two user reports is outstanding.
- 2026-09-27 delivery update: The native 19-track rerender found that the
  unchanged-tone safety comparison crossed the provisional −40 dB duplicate
  threshold on track-a and track-g, creating two small extra deliverables per
  mix. The user wants fundamental processing integrated into strong complete
  masters, not a separate version for its absence. The fallback remains a
  complete measured trial with status `comparison-only` when distinct and
  `duplicate` when below the screen; native and cloud finalizers both suppress
  it from delivered versions. This changes delivery selection only, not DSP.
  A native/distributed regression checks the same behavior. The in-progress
  private corpus render used the earlier delivery selector; its treatment audio
  remains valid for measuring the foundation, and any extra fallback files are
  identified separately in the run report.
- 2026-09-27 completed corpus update: All 19 native sets and independent WAV
  audits pass. The corrected visible family identities match the earlier 94
  versions exactly; each set selected one to four local regions. Essential's
  median matched residual from the earlier set is −40.9 dB, its largest
  two-second level movement across tracks is 0.140 dB, and its largest absolute
  delivered-level difference is 0.029 LU. Denser's corresponding median
  residual is −41.0 dB and maximum level difference is 0.004 LU. These
  measurements establish bounded effect size and level preservation, not
  listening benefit. Two complete comparison-only fallback renders cost a
  median 79.4 s of per-track work; unpaired total render elapsed time rose by
  a median 39.2 s, while direct treated-family times did not show a clear
  increase. The final
  track-a selector rerender retains four family versions with byte-identical
  treated PCM to the pilot. No no-op case occurs in this corpus, so the
  unchanged path and thresholds require contrasting-source listening review.
- 2026-09-27 deployment update: The shared Rust engine and distributed
  finalizer were deployed to the US East 1 alpha stack with matching browser
  WASM. The new image reached `UPDATE_COMPLETE`; a fresh source-bound
  synthetic FLAC plan returned two verified WAVs byte-identical to local
  native output, and the finalizer retained both. This validates release
  wiring and source-only parity, not the foundation's musical quality or
  full-song cloud latency.
- Review condition: Compare same-density complete masters at matched and
  delivered level across diverse and unfamiliar recordings; inspect temporal,
  tonal, stereo and limiter behavior. Revise or withdraw thresholds on audible
  harm, missed useful regions, unnecessary near duplicates or transition faults.
- Extends D065 from offline proposal to provisional production behavior; replaces
  D062's vocal-only automatic coverage limitation while retaining its diagnosis.

## D067: Publish verified Essential and Denser before cloud finalization

- Recorded: 2026-09-27.
- Origin: The user wants quicker listening while masters render and prioritizes
  Denser after Essential, ahead of narrower focus versions.
- Status: Accepted for the cloud prototype; implemented in the Rust family and
  control Lambdas and the browser's cloud set receiver.
- Decision: Once the Essential family writes and verifies each complete PCM24
  WAV, upload Essential immediately. Upload Denser next only when its candidate
  rule retains it and its matched residual from Essential passes the same
  provisional −40 dB duplicate screen as finalization. Publish a source-bound,
  verified preview marker after each immutable WAV upload. The owner-bound
  control status returns those markers in Essential, Denser order with fresh
  signed URLs. The browser checks the WAV hash, stores it as a new local file,
  and exposes LISTEN while later families render. Finalization reconciles the
  same IDs and hashes, then supplies measured labels, reports and delivery actions.
- Reason: Complete verified versions can be heard before other families finish.
  The Essential family renders in that order, so Denser follows naturally.
- Alternatives: Streaming PCM chunks before song-wide loudness and limiting
  settle could expose audio that disagrees with final delivery. Waiting for all
  families delays first listening. D056's indexed Opus and on-demand WAV plan
  addresses transfer size separately and remains proposed.
- Evidence and limits: The existing distributed worker already writes and
  verifies each candidate in sequence; this change advances publication only.
  The browser still downloads and decodes the full selected WAV before playback,
  so first-sound latency and network use need measurement on full songs. The
  duplicate threshold is provisional and should be reviewed with listening.
- Verification: Regression tests cover callback order, source-bound verified
  markers, fallback suppression, and preview/final hash agreement. Live AWS
  timings and release identity belong in the cloud run.
- 2026-09-27 deployment update: ECR image
  `sha256:7a02ea0ff2116b3f212b5e5d515297879a5257087307f84755db54f88642ec83`
  reached `UPDATE_COMPLETE`. In a six-second live synthetic job the control
  status first exposed Essential at 8.348 s, Denser at 11.364 s and the final
  manifest at 13.560 s. Both preview WAV hashes matched native and final
  output exactly. A disposable browser protocol run saved Essential and Denser
  once each, exposed them in that order, and replaced their preview metadata
  with final labels without redownloading audio. Full-song browser first-sound
  timing remains unmeasured.
- 2026-09-27 replacement update: [D072](#d072-run-cloud-set-planning-and-completion-within-aws) moves planning and finalization triggers to AWS and adds waveforms to preview markers. [D073](#d073-stream-one-listening-version-and-place-transport-in-the-player) replaces full-track `AudioBuffer` decoding with one media stream. The earlier deployment result and timings remain historical.
- 2026-10-16: The preview marker (`mastering.preview.2`, `preview_marker` in the Lambda entry module) also carries the `clipping`, `restoration` and `compressor` reports and the `drive_db` of the render, beside `limiter` and `limiter_trace`. Its `delivery` gives `integrated_lufs` and `peak_estimate_dbtp`. The control Lambda gives the browser the `report` of the marker unchanged. The detail view of the MASTER screen reads these fields before the final manifest arrives (bitneedle-app `273b765`). Essential can carry the compressor of `set.compression.v2`, so the marker carries its report too. Test: `tests::a_preview_marker_carries_the_render_report_of_the_detail_view` (bootstrap). Deployed on 2026-10-16.
- 2026-10-16: On the deployed stack, the Essential preview of a split-first job of track-c (2:54) was stored 126 s after the submit and the manifest 216 s after it. For a job of regression mix 2 (2:31) with four saved stems, the times were 61 s and 118 s.
- Review condition: Revise the preview gate if listening finds useful Denser
  candidates below the duplicate threshold, or if first-sound latency remains
  too high on real tracks. Measure D056's compressed preview path separately.
- Extends [D056](#d056-cloud-previews-and-on-demand-lossless-delivery) for early
  candidate availability without adopting its proposed codec or retention policy.

## D068: Compact the listening controls and move reports into version menus

- Recorded: 2026-09-27.
- Origin: The user found the delivered-level status sentence redundant and
  REPORT too prominent in each master card.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: Place a LOUDNESS heading in CUT's accent-label style above one row
  of Matched, Delivered and an accessible icon Play/Pause button. Reserve the
  status line for loading and errors. Put REPORT
  in each completed master's “…” menu beside its level; its content still
  opens inline with COPY REPORT. Early previews expose LISTEN only.
- Reason: Playback level and playback action are used during comparisons;
  JSON evidence is available without competing with primary version actions.
- Alternatives: Retain a full-width text Play button or a REPORT button on
  every card. Both consumed space needed for the comparison controls.
- Evidence and validation: A disposable 320 px Chromium check found the three
  controls on one row, a 44 px icon target, no horizontal overflow, and no
  empty status sentence. Syntax, web performance check and distribution build
  passed. Musical listening is unchanged and was not reevaluated here.
- 2026-09-27 interface update: The user asked that LOUDNESS occupy its own
  well. Group its accent heading and playback-level row in a bordered plate
  within the shared player, separate from version choice and waveform.
- 2026-09-27 interface update: Close an open version “…” menu when a pointer
  press lands outside it. Remove the document listener when MASTER closes.
  This prevents a report menu from obscuring later comparisons. A disposable
  Chromium check confirmed outside dismissal and reopening REPORT.
- 2026-09-27 replacement: [D071](#d071-separate-play-from-the-loudness-well)
  replaces the grouping of Play/Pause inside LOUDNESS.
- Review condition: Revisit if accessible playback or report discovery fails
  in real use or if narrower supported viewports force wrapping.
- Extends [D067](#d067-publish-verified-essential-and-denser-before-cloud-finalization)
  and the browser comparison interface.

## D069: Show measured master changes beside the report

- Recorded: 2026-09-27.
- Origin: The user asked for an interactive, GPU-accelerated way to visualize
  completed masters from their reports.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: On opening a completed version's REPORT, measure its stored WAV
  against the aligned original in a worker and cache a separate analysis file
  in that local mastering set. Display a time-by-log-frequency change map at
  matched or delivered level, with full, mid and side views, zoom, inspection,
  processing-frequency markers and seek into the shared comparison player.
  Keep the original JSON report accessible below it. Render with WebGL2 and
  retain a Canvas 2D fallback.
- Reason: Existing reports contain processing settings and whole-song measures,
  but lack a time-frequency measurement. Comparing the immutable rendered WAVs
  works for both existing and future local/cloud sets without changing DSP or
  source audio. Matching integrated loudness makes tonal differences visible;
  the delivered view shows the full level change.
- Alternatives: Draw a map directly from selected EQ settings or run live FFT
  during playback. Settings do not describe the output spectrum, while live
  analysis would omit unplayed passages and make seeking less predictable.
- Evidence and validation: The worker reads eight 2048-frame stereo windows in
  each of 256 time columns and measures 64 log-frequency bands. A disposable
  Chromium check measured a synthetic 3 kHz boost as 6.02 dB with a float32
  source and PCM24 master; WebGL2, Canvas fallback and 320 px layout passed.
  The map samples passages and can miss brief transients. It depicts output
  differences, not causal contributions from individual processors. Real-track
  usefulness and listening synchronization still require review.
- Review condition: Revisit time sampling, frequency resolution and colour
  range if real-track listening shows misleading or hidden changes; consider
  a fuller analysis sidecar if brief passages matter to decisions.
- Extends [D068](#d068-compact-the-listening-controls-and-move-reports-into-version-menus).

## D070: Give MUSIC search the masthead slot

- Recorded: 2026-09-27.
- Origin: The user found that the MUSIC page title displaced Search.
- Status: Accepted; implemented in the BITNEEDLE web MUSIC route.
- Decision: Omit the root MUSIC breadcrumb and its empty wrapper from the
  masthead. Keep Search in the left slot. Retain the path and Back action on
  nested track screens.
- Reason: The selected MUSIC navigation stamp already identifies the page.
  Search is the primary control in the root library masthead.
- Alternatives: Move Search below the masthead or reduce the title size.
  Both keep the root title and move or constrain the main search control.
- Evidence and validation: Code inspection found the root path inserted a
  MUSIC breadcrumb before the Search host. Disposable desktop and mobile
  layout checks verify Search placement and nested navigation.
- Review condition: Revisit if Search or Back loses its expected position on
  a supported viewport.
- Extends the root MUSIC layout decision in the web app layout document.

## D071: Separate Play from the loudness well

- Recorded: 2026-09-27.
- Origin: The user found that the well grouped Play with loudness controls.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: Put only Matched and Delivered inside the LOUDNESS well. Give the
  well a small corner radius. Put the icon Play/Pause button beside the well,
  aligned with the level buttons.
- Reason: The well identifies the controls that change listening level. Play
  controls transport and must read as a separate action.
- Alternatives: Keep Play inside the well or move it to another row. The first
  misgroups the action; the second adds height to the comparison player.
- Evidence and validation: Disposable narrow and desktop Chromium checks
  verify the separate button, 44 px target, and one-row placement.
- Review condition: Revisit if the well or Play target fails at a supported
  viewport width.
- Replaces the Play grouping in [D068](#d068-compact-the-listening-controls-and-move-reports-into-version-menus).
- 2026-09-27 replacement update: [D073](#d073-stream-one-listening-version-and-place-transport-in-the-player) moves Play beside the selected version above the waveform. The separate placement here is historical.

## D072: Run cloud set planning and completion within AWS

- Recorded: 2026-09-27.
- Origin: The user wants mastering to continue after navigating away and wants stored stems sent as lossy Opus estimates when present.
- Status: Accepted; implemented in the AWS planner, control and family Lambdas and the BITNEEDLE cloud client. Live full-track validation remains pending.
- Decision: A cloud job uploads the preserved FLAC original once. If all four aligned library stems are ready, it also uploads their existing 192 kb/s SoundKit Opus streams in fixed drums, bass, other, vocals order. The native Rust planner decodes and checks those streams, computes the same bounded source/stem evidence and source-bound family plan, and publishes the original comparison WAV. With no saved stems it plans from the original stereo mix. Each family renders that original mix, and the last completed family invokes finalization without a browser status request. Signed-in polling only reports state and downloads immutable, hash-checked files. The UI changes CANCEL to LEAVE once submitted and can resume collection later. Local-only mode retains the WASM renderer.
- Reason: Browser planning and status-triggered finalization stopped when its page closed. The client persists the completed-upload handoff before calling submit, so reopening can retry that idempotent request if navigation interrupts its response. Uploading the stored Opus estimates avoids sending four FLAC stem copies or re-encoding; the original remains lossless. Source-only analysis uses full-mix frequency and dynamics evidence when stems are absent, while stem-specific estimates cannot be inferred from the original alone.
- Alternatives: Keep client WASM planning and split on the browser (page lifetime and memory cost); upload lossless stems (larger transfer without proven improvement in bounded evidence); render the Opus stems (would violate original-mix rendering).
- Evidence: The library stores `stream.sk2` Opus beside `lossless.sk2` FLAC; `stem_evidence::analyze_set` checks role alignment, reconstruction and source binding. A synthetic non-packet-aligned Opus stream decoded with 423 extra tail frames; the planner trims only a tail shorter than one 20 ms packet to the source-derived frame count before these checks. The existing controller invoked finalization only from status. The new planner and family invoke chain removes that dependency. The original and each preview receive measured 384-bin waveforms.
- Rule and code references: the Lambda planner, source, control and entry modules (`source.rs`, `control.rs`, `main.rs`), the stack template and the web mastering module (`bitneedle-app/web/js/mastering.mjs`).
- Limits and counterexamples: Existing stem estimates are lossy measurements and may fail reconstruction or alignment validation. No new separation or stem backfill is performed for a track without stored stems; server-side separation belongs in stemsplits and remains planned. The one-day S3 retention still applies. A full-length signed-in cloud listening and latency run has not yet been completed.
- Verification: Rust Lambda codec/protocol release tests, strict Clippy, the engine release suite, the fixture's four-Opus native planning path, and the web distribution/performance checks passed. Two owner-bound live AWS fixture jobs, one source-only and one with four Opus streams, completed without status polling; Essential preview/final hashes and 384-bin waveforms passed. A real signed-in browser navigation/resume and full-song listening run remain before production validation.
- Review condition: Revisit stem upload bitrate after blind and reconstruction comparisons, and add stemsplits-owned cloud separation/backfill when its whole-track job API is available.
- 2026-09-27 replacement update: [D074](#d074-require-stems-before-every-cloud-plan) removes source-only cloud planning and generates missing stems in AWS. The source-only policy above is historical.
- 2026-09-27 interface update: [D075](#d075-explain-the-cloud-handoff-without-a-leave-control) removes LEAVE and explains the upload and background handoff. The LEAVE control above is historical.
- Supersedes the cloud portion of [D067](#d067-publish-verified-essential-and-denser-before-cloud-finalization) that depended on browser planning/polling; extends [D061](#d061-require-flac-for-mastering-and-source-delivery).

## D073: Stream one listening version and place transport in the player

- Recorded: 2026-09-27.
- Origin: The user reported Safari restarting when switching versions and asked for Play inside the player, short action names, an overload-style progress scale and an Essential preview waveform.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen; Safari device validation pending.
- Decision: Use one HTML audio stream backed by one local WAV object URL for comparison, pausing, unloading and revoking the previous URL before opening another. Feed that stream through one Web Audio gain node for matched loudness, including Safari devices that ignore media-element volume. Put Play beside the current version name above the waveform, outside the LOUDNESS well. Use short button labels, with the full version label and explanation in its row. Show a 24-light progress scale with green and yellow during work, then four overload-red lights at completion. Include the measured preview waveform in the verified Essential/Denser marker and status record.
- Reason: The previous `decodeAudioData` path could hold two full-track decoded `AudioBuffer`s plus WAV bytes during a switch. A single media element leaves buffering to Safari and releases the previous URL explicitly. The visual meter communicates workflow progress; its dB markings are visual labels, not an audio peak measurement.
- Alternatives: Keep decoded buffers and rely on garbage collection (Safari termination risk); decode short excerpts only (would limit seeking); put Play in the loudness well (groups transport with level controls).
- Evidence: Code inspection found `decodeAudioData(await blob.arrayBuffer())` during every switch and no explicit old-buffer reclamation. The cloud preview marker omitted waveform even though final versions already included it. Rust computes the same absolute-scale 384-bin waveform for previews.
- Verification: Browser syntax and a disposable WebKit check loaded a float WAV original, switched to a PCM master and back, then confirmed Play and version switching with the gain graph. Safari switching on a full-length set and instrumented memory measurement remain required; the memory hypothesis is not yet confirmed on device.
- 2026-09-27 deployment update: The deployed BITNEEDLE web Worker served the new comparison player, MASTER controls and cloud upload module at `yl.vin`; live asset reads confirmed the deployed code. A real signed-in track check and Safari device memory run remain pending.
- Review condition: Measure peak page memory and playback handoff on Safari; adjust metadata loading and seeking if media-element behavior is inconsistent.
- Replaces the Play placement in [D071](#d071-separate-play-from-the-loudness-well).

## D074: Require stems before every cloud plan

- Recorded: 2026-09-27.
- Origin: The user explicitly requires planning from stems on every mastering job.
- Status: Accepted; implemented in the cloud protocol and stemsplits whole-track worker. Deployment and live validation are recorded separately.
- Decision: A cloud plan always receives four aligned drums, bass, other and vocals estimates. Saved library Opus streams are uploaded when available. Otherwise the stemsplits AWS worker decodes the uploaded lossless original, applies the same SplitSession chunk and seam contract as the browser, calls the existing model segment service, writes four SoundKit Opus estimates and invokes the planner. The planner rejects zero or partial stem keys. All mastering versions still render the original stereo mix.
- Reason: Source-only planning misses role-specific bass, drums, accompaniment and vocal evidence that the engine uses for bounded decisions. A page can close after upload without stopping cloud separation or mastering.
- Alternatives: Run browser separation before cloud upload, or plan without stems. Browser work would depend on page lifetime; source-only analysis cannot recover role-specific evidence.
- Evidence: D072's source-only job proved technical completion but did not provide stem evidence. `SplitSession` already defines segment offsets and overlap-add in stemsplits. This change reuses it on AWS. Existing stem evidence validation checks alignment and reconstruction before a plan is accepted.
- Rule and code references: the Lambda control and planner modules (`planner.rs`), the stack template, and the stemsplits whole-track worker (`stemsplits/deploy/aws/wholetrack`).
- Limits and counterexamples: Model estimates can be inaccurate; validation checks alignment and reconstruction, not perceptual separation quality. The cloud-generated estimates currently remain in the short-lived AWS job and are not yet imported into the permanent track library. Local-only mode is a separate on-device workflow.
- Verification: Rust fmt, strict Clippy and release tests passed for the engine and Lambda crate; stemsplits whole-track tests passed. The deployed US East 1 stack completed a six-second and a 192.94-second source-only split-first job without browser polling. Each stored four Opus stems, a stem-based plan and a final manifest with Essential and Denser. On the full track the last stem appeared at 402 s, the plan at 417 s and the manifest at 574 s after submit. Essential's preview and final hashes matched and both final waveforms had 384 bins. A signed-in browser navigation/RESUME and full-length listening run remain pending.
- Review condition: Check complete-song latency, separation cost, estimate quality and failure handling before production promotion.
- Replaces the no-stems cloud behavior in [D072](#d072-run-cloud-set-planning-and-completion-within-aws).

### Updates

- 2026-10-16: The track library keeps the stems that the cloud makes. The planner records the role, SHA-256 and size of each stem that it reads in `source.json` (`stems`). For a job that came without stems, the `complete` status of the control Lambda gives the four stems with those fields and a signed link (`cloud_stem_records` in `control.rs`). When the set is complete, bitneedle-app (`280d581`, `takeCloudStems` in `web/js/stems.mjs`) checks each download against its SHA-256, decodes it to a 16-bit WAV and adds it to the library as a split adds its stems. The next cloud master of the track uploads the four saved Opus stems. A track that has stems keeps them, and a failed or stopped import leaves the complete set as it is. Tests: `tests::only_the_stems_that_the_cloud_made_are_offered_to_the_library` (control) and `tests::a_stem_record_gives_the_hash_and_size_of_the_object` (planner). A disposable headless check of `/master` against a mocked API, from the source and from the built site, added the four stems of regression mix 2 to the library with their streams. Deployed on 2026-10-16. On the split-first job `11d93e24`, the app downloaded the four stems and started to add them to the library.
- 2026-10-16: Live checks from the public site. A split-first job of track-c stored its four stems 58 s after the submit and its manifest 216 s after it, with no page open. A job with four saved Opus stems from the browser invoked the planner directly and stored its manifest 118 s after the submit. An anonymous account cannot split in the cloud ("Sign in to split this track."); the split on the device made the four stems of that job.

## D075: Explain the cloud handoff without a leave control

- Recorded: 2026-09-27.
- Origin: The user wants mastering to continue in the background after leaving the page, with a clear note and no LEAVE button.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: Keep the page open while it uploads the mix and any saved stems. After the upload, save a resumable local job record; once the AWS submit request succeeds, hide CANCEL and say the listener can leave and return. A return to the same device's track offers RESUME, which retrieves stored results or retries an interrupted submit. Do not show a LEAVE button.
- Reason: The browser must transfer local audio before AWS can work independently. S3 holds the submitted marker, stem estimates, plan and rendered files after handoff; page navigation only stops local polling and download.
- Alternatives: An explicit LEAVE control added a step with no server effect. Claiming the job is running as soon as upload completes would be premature until AWS accepts submit.
- Evidence and verification: A direct AWS split-first job completed with four stored stems, a plan and a final manifest without browser polling. Disposable mobile layout inspection and live asset reads confirmed the note and removal of LEAVE. A signed-in browser navigation/RESUME run remains to be done.
- Limits: RESUME discovers jobs from the local track library, so it currently applies to the same device and browser storage. A page must stay open until the upload finishes. S3 jobs expire after one day.
- Review condition: Revisit if signed-in navigation loses a job after submit, or if server-side account job listing is added for cross-device resume.
- Replaces the LEAVE control in [D072](#d072-run-cloud-set-planning-and-completion-within-aws).
- 2026-09-27 replacement update: [D080](#d080-resume-pending-mastering-automatically-on-return) removes manual RESUME. Its earlier action is historical.

## D076: Let the mastering player sit on the page

- Recorded: 2026-09-27.
- Origin: The user asked to remove the first box around loudness and playback.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: Remove the comparison player's outer border and padding. Keep the separate, slightly rounded LOUDNESS well around its level controls.
- Reason: The player reads as part of the page while the loudness control remains a distinct setting.
- Alternatives: Keep the full player box or remove the loudness well too. Both conflict with the requested grouping.
- Evidence and verification: A disposable 390 px Chromium inspection measured zero outer border and padding and a 1 px border on the 6 px radius loudness well. The public asset deployment is recorded with the web release.
- Review condition: Revisit if the unboxed player loses clear spacing or alignment on supported screens.

## D077: Draw one-sided peak and RMS in the mastering player

- Recorded: 2026-09-27.
- Origin: The user questioned whether the mirrored waveform gives useful information.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: Draw the existing positive peak and RMS bins once above a baseline in a 48 px player waveform. Retain the shared playhead and seek control.
- Reason: The stored bins are scalar magnitudes and the former upper and lower polygons were exact mirrors. The second half consumed height without showing polarity or separate stereo channels.
- Alternatives: Keep the 80 px mirror, or add true signed/channel envelopes. The former is redundant; the latter requires different data and a separate listening use case.
- Evidence and verification: The previous player mapped each `bin.peak` and `bin.rms` to equal distances above and below its centerline. A disposable viewport check verifies the compact SVG and controls after the change. The measured audio bins are unchanged.
- Review condition: Revisit if the player gains signed extrema or channel-specific waveforms that warrant two distinct shapes.

## D078: Version complete mastering sets and suppress repeat renders

- Recorded: 2026-09-27.
- Origin: The user wants a new MASTER action only when the engine has a fresh version for that track.
- Status: Accepted; implemented in the BITNEEDLE web client.
- Decision: Use the Rust `mastering-engine` package version as the release ID. Store it and stem-planning provenance in every new local or cloud set; show the release in set history, reports, downloaded master filenames and masters added to Music. Hide MASTER when the track has a complete stem-planned set or a pending cloud job from that release within its one-day S3 retention period. Keep older sets available. A set without a version or stem provenance is treated as an earlier release. The web build checks its release ID against the Rust package version.
- Reason: A completed set already contains every candidate accepted by that engine release; repeating it wastes time and cloud resources. Old sets remain useful for comparisons after a new release.
- Alternatives: Compare timestamps or count only visible versions. Neither identifies the rules that made the versions; candidate counts can legitimately differ by track.
- Evidence and verification: `summary.json` already marks complete only after all accepted versions and verification are saved. Current stored sets do not have an engine ID, so their exact release cannot be inferred. Browser checks cover fresh, complete, pending and older sets; a signed-in cloud revisit remains to be checked.
- Limits: The package version must be bumped for material DSP, planning or candidate-selection releases and deployed with the matching web version. A report records the client release ID; existing AWS manifests have no independent engine-version field.
- Review condition: Add a server-declared version to AWS manifests if release skew appears, or when cross-device job lists are introduced.

## D079: Require stems for local-only mastering too

- Recorded: 2026-09-27.
- Origin: The user's requirement to always plan from stems applies to the optional local workflow as well as AWS.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: Local-only MASTER requires four completed library stems before passing audio to the WASM engine. An in-progress four-stem import may finish before rendering; with no stems the screen directs the listener to STEMS. A completed source-only set is not considered current for the repeat-render gate. Native engine APIs still accept source-only inputs for controlled regression and fallback analysis.
- Reason: The product should not silently omit bass, drums, other and vocal evidence when a listener explicitly requests mastering. The separate native source-only path remains useful for diagnostics and preserves unchanged processing.
- Alternatives: Automatically start the existing browser stem split or keep source-only local planning. Browser splitting would depend on page lifetime and service availability; source-only planning violates the product requirement.
- Evidence and verification: The prior local branch returned an empty stem list when no library stems existed. The new client guard refuses that case before it creates a mastering set. Disposable branch checks and app syntax/build validation passed; a real local four-stem render remains to be exercised.
- Review condition: Revisit automatic local splitting when stemsplits offers an on-device path with acceptable latency and persistence.
- Extends [D074](#d074-require-stems-before-every-cloud-plan) to the optional local workflow.

## D080: Resume pending mastering automatically on return

- Recorded: 2026-09-27.
- Origin: The user does not want a RESUME action or repeated infrastructure language in the UI.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: On opening a track's MASTER screen, the client reads its pending, unexpired local job records and resumes their AWS submission, status retrieval and verified downloads in order. If sign-in expired, it opens the sign-in flow and returns to the track. Remove RESUME and LEAVE buttons. The handoff note says only that mastering continues and the listener can leave and return. UI copy uses separate sentences without semicolons.
- Reason: The app already holds the job ID and can perform the next step itself. The listener should only need to keep the page open until upload finishes.
- Alternatives: Retain an explicit RESUME action or poll every job on every app route. The first adds unnecessary work; the second creates background network activity away from the track.
- Evidence and verification: The client persists `submitStartedAt` before calling submit and AWS retry submission is idempotent. Direct short and full-song AWS jobs completed without browser polling. A disposable Chromium return check created a pending OPFS job, mounted MASTER and observed automatic token request with no RESUME button. Expiration and release gating received branch checks. A real signed-in browser navigation and return remains to be verified.
- Review condition: Revisit retry cadence and cross-device discovery when account-level job listing is available.
- Replaces the manual RESUME behavior in [D075](#d075-explain-the-cloud-handoff-without-a-leave-control).

### Updates

- 2026-10-16: Returns to two jobs on the deployed site after the page closed at the submit. Each return resumed by itself, downloaded the verified files and said "4 versions ready and added to Music." Three faults: a download that stops receiving bytes waits with no time limit ("Saving Essential…" for 9 minutes); a page that closes during a download leaves an empty WAV, and the next return stopped with "Master version already exists." (bitneedle-app `c433d57`, deployed on 2026-10-16, downloads such a file again); after a cancel or a completed return, the note under the status keeps its earlier text.

## D081: Flicker the leading mastering LEDs during work

- Recorded: 2026-09-27.
- Origin: The user wants the upper progress lights to show that mastering is still working.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: Animate opacity on the leading lit LED and nearby orange LEDs while a job is active. Keep red overload lights for completion. Stop animation on completion, cancellation and page exit; disable it under reduced motion.
- Reason: The current LED scale can look frozen during multi-minute separation and rendering, even while status text changes.
- Alternatives: Animate the whole meter or cycle through unrelated lights. Both would imply progress the system has not measured.
- Evidence and verification: The LED state is tied to the existing measured progress fraction and active job state. A disposable Chromium check read `master-led-front` while working and `none` after stopping or under reduced motion. Real-device motion review remains pending.
- Review condition: Adjust timing if the signal distracts from listening or appears to show false progress.
- 2026-09-27 replacement update: [D082](#d082-keep-the-led-scale-contiguous) removes the scattered orange LED animation. The earlier animation is historical.

## D082: Keep the LED scale contiguous

- Recorded: 2026-09-27.
- Origin: The user observed a middle light brighter than its neighbours and wants the meter to read as a scale.
- Status: Accepted; implemented in the BITNEEDLE web MASTER screen.
- Decision: All lit progress LEDs remain steady and contiguous. Only the first unlit LED after the measured progress frontier may flicker faintly while work is active, in green or orange according to its position. It is always dimmer than a lit LED. Red overload LEDs illuminate only on completion. Reduced motion disables the flicker.
- Reason: Animating alternating lit LEDs creates isolated bright spots that make the meter look random instead of ordered.
- Alternatives: Animate several lit LEDs or pulse the entire lit range. The former breaks the visual scale; the latter can suggest a progress change that has not occurred.
- Evidence and verification: The frontier is derived from the existing measured progress fraction. A disposable Chromium check confirmed that only its next unlit LED animates, remains dimmer than the lit scale, and stops under reduced motion. Real-device visual review remains pending.
- Review condition: Revisit the frontier animation if it is too subtle to indicate activity or if the meter still suggests unmeasured progress.
- Replaces the animation choice in [D081](#d081-flicker-the-leading-mastering-leds-during-work).

## D083: Give the active workflow path word a solid background

- Recorded: 2026-09-27.
- Origin: The user pointed out that DETAILS in MUSIC / DETAILS lacks a background on a track page.
- Status: Accepted; implemented in the BITNEEDLE web client.
- Decision: Render the active word of a workflow breadcrumb on a compact solid ink block with ground-colored text in all themes and viewport sizes. Earlier path words remain navigation links.
- Reason: The active screen label should read as a deliberate part of the poster-style interface rather than unstyled text.
- Alternatives: Add a background to the entire breadcrumb row or make the active word a clickable button. The first would obscure the path hierarchy; the second would present a control with no destination.
- Evidence and verification: The shared workflow path renders the final word with `aria-current="page"`. A disposable Chromium style check confirmed solid, contrasting background and text in dub, light and dark themes. Visual review on the signed-in track page remains pending.
- Review condition: Revisit padding or color if the label feels too heavy beside longer paths.
- 2026-09-27 clarification: [D084](#d084-fill-the-master-screen-back-link) covers the distinct ‹ DETAILS control in the fixed masthead.

## D084: Fill the MASTER screen back link

- Recorded: 2026-09-27.
- Origin: The user pointed out that ‹ DETAILS on a track's MASTER page still had no background after the active breadcrumb changed.
- Status: Accepted; implemented in the BITNEEDLE web client.
- Decision: Give the fixed masthead's back link an opaque page-colour background and normal ink text, including its chevron. Apply it across themes and viewports.
- Reason: ‹ DETAILS is a separate navigation control from the active breadcrumb word. It needs a page-colour patch so the checkerboard behind the masthead does not show through.
- Alternatives: Fill the whole masthead row. That would obscure the distinction between the back control and nearby content.
- Evidence and verification: The shared `workflow-back-link` appears in the MASTER masthead. The original ink-fill style was deployed and the user rejected its colour. A disposable Chromium style check confirmed the corrected background equals `--ground` with contrasting text and chevron in dub, light and dark themes. The signed-in track page still needs visual review.
- Review condition: Revisit if the back control crowds the masthead on narrow screens.
- 2026-09-27 expansion: [D085](#d085-keep-navigation-words-opaque-over-the-masthead-pattern) applies the same page-colour surface to other navigation controls and PNG.

## D085: Keep navigation words opaque over the masthead pattern

- Recorded: 2026-09-27.
- Origin: The user wants the page-colour background on all navigation, a background for PNG, and pink text on hover.
- Status: Accepted; implemented in the BITNEEDLE web client.
- Decision: Use opaque `--ground` behind masthead path links, back and forward navigation, CUT session controls, and the ART PNG action, including its disabled state. Interactive navigation text turns `--wavey-pink` on hover. Keep the active path word's existing ink block.
- Reason: The checkerboard should frame the masthead controls without showing through their words or buttons.
- Alternatives: Fill the entire masthead row or make every control ink-filled. Both would remove the separate poster-like patches the user requested.
- Evidence and verification: A disposable Chromium check confirmed page-colour backgrounds and pink hover text for representative controls at mobile and desktop widths in dub, light and dark themes. Signed-in visual review remains pending.
- Review condition: Revisit if the patches form an unwanted continuous bar or if pink loses contrast in a future theme.

## D086: Show selected mastering controls and measured limiter activity

- Recorded: 2026-09-27.
- Origin: The user rejected the sampled frequency-change report and asked for an EQ and limiter view resembling a music plugin.
- Status: Accepted; implemented in the engine telemetry and BITNEEDLE comparison player, with live listening review pending.
- Decision: Remove the sampled output-difference map and its cached analysis. Display the selected version's actual tone EQ filter response at the playback position and the limiter's measured mean and maximum gain reduction in 384 time bins. Keep the raw JSON report for inspection and copying. Versions without a gain trace display aggregate limiter measurements when available, without inventing a time history. Release this report protocol as engine 0.1.1 so existing complete sets may be remastered when desired. The waveform treatment is specified in [D088](#d088-add-a-frequency-coloured-mastering-waveform-with-time-zoom).
- Reason: The sampled output difference mixed processor effects and could not identify which control changed. A filter-response curve and gain-reduction trace answer the processing question directly.
- Alternatives: Continue measuring a spectral output-difference heatmap or infer limiter activity from the delivered waveform. Neither represents the selected processor settings or measured gain reduction.
- Evidence and verification: The EQ curve uses the serialized selected tone filters and source focus envelopes. The limiter trace is accumulated from the limiter gain during the selected Rust render; a release regression test checks its bin alignment and agreement with whole-song statistics. The view is descriptive of controls and limiter action; it is not an estimate of the final audible frequency balance. Browser and live AWS verification remain pending.
- Review condition: Revisit the display after listening review, especially whether finishing and dynamic band gain needs separate measured time traces.
- 2026-09-27 update: [D101](#d101-make-the-mastering-visuals-readable-without-a-panel) refines the browser presentation and empty states after the user found the initial plots blank. The serialized processing measurements and Rust rules remain as recorded here.

## D087: Use three fixed slots per mastering version row

- Recorded: 2026-09-27.
- Origin: The user wants ORIGINAL, DENSER and the other version buttons aligned left in groups of three even when fewer versions exist.
- Status: Accepted; implemented in the BITNEEDLE comparison player.
- Decision: Use three equal grid columns for version choices at every viewport width. Leave unused cells empty and wrap additional versions to subsequent rows.
- Reason: Button size and placement should stay stable as a job delivers previews and final versions.
- Alternatives: Flexible buttons expanding to fill each row. That made one or two versions change size during rendering.
- Evidence and verification: CSS grid enforces three columns and source order. Disposable Chromium checks at 390 px and 1,360 px verified one, two, three, four, and eight buttons retain equal widths and wrap after the third. Signed-in visual review remains pending.
- Review condition: Revisit text size if a future version name cannot fit a narrow three-column phone layout.

## D088: Add a frequency-coloured mastering waveform with time zoom

- Recorded: 2026-09-27.
- Origin: The user clarified that the Music-style coloured waveform belongs in the mastering column as well, and that a loud master can look like one solid block at full-song scale.
- Status: Accepted; implemented as independent BITNEEDLE WAV analysis, with live review pending.
- Decision: Read each saved original or master WAV in bounded chunks and cache a 2,048-bucket three-band waveform beside the set. Split at 200 Hz and 2 kHz using Music's one-pole visual convention, merge the channel magnitudes, and draw one centred, mirrored waveform whose colour shows the band balance. Provide horizontal time zoom from 1× to 32× around the playhead with click-to-seek inside the visible window. Show one peak envelope while analysis is pending. Any existing master can receive this waveform without a new render or engine protocol change.
- Reason: Source and mastered audio can differ in frequency balance; reusing only the source sidecar for every version would misdescribe a master. Horizontal zoom reveals passages otherwise compressed into a few pixels at whole-song scale.
- Alternatives: Colour the old unbanded amplitude envelope, replay the source sidecar for masters, or add this summary to the mastering engine's render protocol. The first two misdescribe the selected audio; the last requires a new render for an existing master. Bounded independent WAV reads avoid whole-song browser audio buffers.
- Evidence and verification: The separate worker reads saved PCM in 8,192-frame chunks and uses the Music sidecar's 200 Hz and 2 kHz one-pole split. A disposable browser analysis of float32 WAV test tones put 70 Hz mainly in bass (121 versus 4 high) and 7 kHz mainly in treble (114 versus 7 bass). The selected canvas and zoom worked in Chromium. Real-device visual review remains pending.
- Review condition: Revisit colour scaling or zoom levels if loud masters still hide useful time detail or if the 2,048-bucket resolution is insufficient on long songs.
- 2026-09-27 correction: The first implementation added waveform data to engine 0.1.1. The user clarified that this is a view of saved audio, independent of rendering, and asked for one DAW-like shape. That engine addition was removed before release.
- 2026-09-27 update: [D102](#d102-draw-master-and-source-audio-with-one-stereo-band-waveform) replaces the initial mono peak visualization after the user found the mastered shape flat and its dark surrounding area distracting.

## D089: Play mastering comparisons at delivered level only

- Recorded: 2026-09-27.
- Origin: The user sees no value in the Matched loudness option in this comparison player.
- Status: Accepted; implemented in the BITNEEDLE player.
- Decision: Remove the Matched/Delivered selector and its loudness well. Play source and master PCM at their stored delivered levels with unity playback gain.
- Reason: The version comparison should reveal the delivered loudness result directly.
- Alternatives: Keep two listening levels. The user explicitly removed that choice.
- Evidence and verification: The comparison worklet outputs saved PCM at unity gain. Disposable Chromium and WebKit playback handoffs stayed playing and advanced their positions. Signed-in listening review remains pending.
- Review condition: Revisit only if a separate controlled listening study requires level matching.

## D090: Switch versions on one sample clock

- Recorded: 2026-09-27.
- Origin: The user requires seamless version switching at the same sample boundary, without a pause or loading clock.
- Status: Accepted; implemented in the BITNEEDLE comparison player, with device listening review pending.
- Decision: Decode bounded windows of each saved WAV and feed one 48 kHz AudioWorklet. Keep the current version playing while the next window loads. The worklet changes versions at one source frame index and blends corresponding frames for 128 samples, then discards the old windows. Seeks and playback use the same worklet frame clock.
- Reason: Two browser media elements have independent clocks and cannot guarantee sample alignment; whole-song AudioBuffers risk Safari memory failures.
- Alternatives: Pause and replace a media element, run parallel media elements with an approximate crossfade, or retain complete AudioBuffers. None meets both the sample-boundary and bounded-memory requirements.
- Evidence and verification: Disposable Chromium and WebKit checks switched between two five-second WAVs while playback and the frame position continued. A direct worklet check observed the switch starting at frame 256, completing at frame 384, and outputting no silent samples. A bounded PCM24 WAV read check covered positive and negative values. Safari device listening remains pending.
- Review condition: Revisit the 128-frame blend if phase differences create audible transition artifacts or if a device shows underflow.

## D091: Highlight the selected version row without moving it

- Recorded: 2026-09-27.
- Origin: The user dislikes the selected row's side margin and layout jump and wants the whole row pink.
- Status: Accepted; implemented in the BITNEEDLE UI.
- Decision: Give every version row the same padding and border geometry. Fill the selected row with pink and dark text without changing its dimensions.
- Reason: Selection should be visible without shifting row content or nearby elements.
- Alternatives: A left border and extra padding, the prior style, moved content on each selection.
- Evidence and verification: Fixed padding and selected background are in the shared card style. Disposable Chromium checks at 390 px and 1,360 px found unchanged row coordinates and dimensions when selection changed and confirmed the selected background is pink. Signed-in visual review remains pending.
- Review condition: Revisit colour contrast if a theme changes the pink token.

## D092: Put record and promotion destinations on the Music/Art shelf

- Recorded: 2026-09-27.
- Origin: The user requested MAKE A RECORD, RELEASE A RECORD, PROMO and PRESAVE in that order, and removal of the matching side menu items.
- Status: Accepted; implemented in the BITNEEDLE app.
- Decision: Place all four links as shared destination rows at the top of MUSIC/ART, in the requested order. Remove RELEASE and PRESAVE from the secondary rail.
- Reason: The shelf is the single entry point for record and promotion work, with each destination visible in sequence.
- Alternatives: Duplicate the destinations in the shelf and rail. That kept two entry points for the same work.
- Evidence and verification: The `/artmusic` route now builds those four rows and links to `/cut`, `/drop`, `/shorts` and `/presaves`. Syntax and web build checks passed. Disposable Chromium checks at 390 px and 1,360 px confirmed four ordered links, their routes, and no rail tool items; live visual review remains pending.
- Review condition: Revisit the ordering if observed task use suggests a different sequence.
- 2026-09-27 update: Removed a stale shell selection call to the deleted side-tool navigation during player-page work.
- 2026-09-27 update: D099 replaces the four shelf links' 3px solid rules with 1px dashed rules; their order and destinations remain as recorded here.

## D093: Give the search row a quiet hatched ground

- Recorded: 2026-09-27.
- Origin: The user requested removal of the search bar bottom border and a subtle hatched background.
- Status: Accepted; implemented in the BITNEEDLE app.
- Decision: Remove the search input's bottom rule, draw a low-contrast diagonal hatch on its shared topbar ground and keep a visible focus outline.
- Reason: The texture identifies the search region without another horizontal rule; the outline preserves keyboard focus visibility.
- Alternatives: Hatch the input alone or remove all focus styling. The former leaves the rest of the search row visually inconsistent and the latter hides focus.
- Evidence and verification: The CSS uses a 5% ink, 8px repeating diagonal gradient. The web build passed. Disposable Chromium checks at 390 px and 1,360 px confirmed the hatch layer and a zero-width bottom border; live visual review remains pending.
- Review condition: Adjust opacity if the hatch is too strong in either theme or on a phone display.

## D094: Give record intake the shared library add control

- Recorded: 2026-09-27.
- Origin: The user asked for YOUR RECORDS to be a plain heading and for the add action to match Add Tracks and Add Images, saying “Add record from an image.”
- Status: Accepted; implemented in the BITNEEDLE app.
- Decision: Remove the record heading's bottom rule. Use the shared library add panel and plus icon with ADD RECORD on the button and “Import record from an image” beneath it.
- Reason: The heading identifies a section, while the add button starts intake; they should use the same visual roles as the other media libraries.
- Alternatives: Keep the oversized record button and navigation-style heading. Both made this shelf behave differently from ART and MUSIC.
- Evidence and verification: The record picker still accepts PNG, TIFF, BMP and WebP. Disposable Chromium checks at 390 px and 1,360 px confirmed the shared button class, zero-width heading border and no horizontal overflow. The button and note copy was corrected after that check. Local screenshot review passed; live visual review remains pending.
- Review condition: Revisit the note layout if mobile widths change.
- 2026-09-27 correction: The initial panel put “ADD RECORD FROM AN IMAGE” on the button and listed formats beneath it. The user specified “ADD RECORD” as the button and “Import record from an image” as the supporting text. The app now uses that copy.

## D095: Give player view controls solid fills

- Recorded: 2026-09-27.
- Origin: The user asked for the player screen's 2D, 3D and RESET buttons to be solid rather than transparent.
- Status: Accepted; implemented in the BITNEEDLE app.
- Decision: Fill each unselected view or reset button with an opaque blend of the current theme ink and ground. Keep the selected view filled with ink, and use a stronger opaque blend on hover or keyboard focus.
- Reason: The controls must read as buttons over either the fixed top bar or the floating player surface.
- Alternatives: Use the theme plate colour. In Dub it matches the ground, so the controls would still appear unfilled.
- Evidence and verification: Disposable Chromium computed-style checks in Dub, light and dark themes confirmed opaque fills for 2D, 3D and RESET. The web build passed; live player visual review remains pending.
- Review condition: Revisit the tint if it competes with player controls or loses contrast in a future theme.

## D096: Show two record players per shelf row

- Recorded: 2026-09-27.
- Origin: The user requested two player previews per row in the record view, aligned from the left.
- Status: Accepted; implemented in the BITNEEDLE Music/Art shelf.
- Decision: Use two equal grid columns for record cards at phone and desktop widths, with each card surface and its information aligned to the start of its cell.
- Reason: The shelf should show adjacent players while keeping a lone or final odd card anchored left.
- Alternatives: A single full-width column or an auto-fill grid. Neither guarantees two cards per row across supported widths.
- Evidence and verification: The preview scales to card width through its existing ResizeObserver. Disposable Chromium checks at 390 px and 1,360 px showed the first two cards sharing a row, the third starting at the left edge of the next row and no horizontal overflow. Live record visual review remains pending.
- Review condition: Revisit the two-column minimum if a narrower supported viewport makes controls illegible.

## D097: Keep record shelf cards to a preview and one artist line

- Recorded: 2026-09-27.
- Origin: After requesting two player previews per row, the user asked to remove the card text and show the player plus one line with artist and heart.
- Status: Accepted; implemented in the BITNEEDLE Music/Art shelf.
- Decision: Use the shared card renderer's shelf variant only for collection cards. It shows the player preview, artist and save heart on one line. The record title, format and hover text do not print there. Keep record actions in a small icon on the preview; other card contexts retain their details.
- Reason: Small two-column cards need one clear identification line, while record management must remain reachable.
- Alternatives: Hide fields with shelf-only CSS or remove actions. A renderer variant gives the card the intended content order; removing actions would strand catalogue, manifest, download and delete from the shelf.
- Evidence and verification: Disposable Chromium checks at 390 px and 1,360 px found no title or format node, artist-only visible text and no horizontal overflow. Local screenshot review passed; live record visual review remains pending.
- Review condition: Revisit the artist fallback and menu icon size with real pressings.

## D098: Keep the player in one column and navigate to record actions

- Recorded: 2026-09-27.
- Origin: The user requested a familiar single left-aligned player column on desktop and mobile, then a DETAILS › top-nav destination that exposes the record's existing action menu as a page.
- Status: Accepted; implemented in the BITNEEDLE Music/Art route.
- Decision: Align the open deck and player to the start of a 400 px maximum column. Put DETAILS › in the player's fixed workflow masthead. Open `/artmusic/<record-id>/details` as a standard destination list for Play Side, Catalog Details, YLVIN Manifest, Download and Delete. Preserve the existing catalogue and manifest URLs and return to Details when reached from it.
- Reason: The player and its actions should use the same narrow workspace and navigational pattern as the rest of Music/Art.
- Alternatives: Centre the player or open an anchored ellipsis dropdown from the top bar. The dropdown would not make Details a linkable page.
- Evidence and verification: Disposable Chromium checks found the deck at the left content edge with no overflow at 390 px and 1,360 px. A temporary local pressing verified DETAILS ›, the five destination rows, the manifest URL and Back to Details without page errors. Real-record playback and live route review remain pending.
- Review condition: Revisit player persistence if moving to Details and back makes record loading disruptive on actual files.
- 2026-09-27 update: Keep record search only on the `/artmusic` index. The opened Player, Details, Catalogue and Manifest pages use their own masthead without search. Live route checks found one search field on the index and none on Player, Details or Manifest; Catalogue uses the same navigation removal as Manifest. Review if a later route adds its own collection search.

## D099: Thin the Music/Art shelf navigation rules

- Recorded: 2026-09-27.
- Origin: The user found the live Music/Art index visually crowded and suggested a thinner dashed rule under its navigation to echo the checkerboard.
- Status: Superseded by D100 before deployment; implemented for the four Music/Art index links in BITNEEDLE commit `56118ab`.
- Decision: Replace each of those links' 3px solid bottom rules with a 1px dashed rule in the current theme ink. Keep their label, chevron, height, spacing and routes.
- Reason: Four consecutive solid pink rules compete with the checkerboard masthead and repeat more visual weight than this short menu needs.
- Alternatives: Add a new dashed divider under the search row. A live style comparison showed that it added another line without reducing the weight of the four existing rules. Changing the shared destination rule would affect other screens beyond this shelf.
- Evidence and verification: Disposable Chromium screenshots of the live page at 390 px and 1,440 px compared the two treatments before implementation. The app web build passed; deployed visual review remains pending.
- Review condition: Revisit if the lighter separators make rows hard to distinguish with actual records below the menu.
- 2026-09-27 update: The user extended the same dashed treatment to all such navigation. D100 replaces the Music/Art-only scope.

## D100: Use the dashed rule for shared destination navigation

- Recorded: 2026-09-27.
- Origin: After reviewing the thinner Music/Art rules, the user asked for the same treatment on all such navigation, app wide.
- Status: Accepted; implemented in the shared BITNEEDLE `workflow-forward-link` style.
- Decision: Use a 1px dashed bottom rule for shared destination and next-stage links, account destinations and record catalogue navigation in both themes and at all viewport widths. Keep their type, spacing and routes.
- Reason: Navigation rows recur across ART, DROP, CUT, Music/Art and account pages; one separator style makes their hierarchy predictable and carries the checkerboard rhythm across the app.
- Alternatives: Keep the override on Music/Art only, leaving otherwise identical links with different rules. Adding a separate divider beneath the masthead was also compared under D099.
- Evidence and verification: The Music/Art treatment was compared in disposable Chromium screenshots at 390 px and 1,440 px. The app web build passed. Deployed Chromium checks found `1px dashed` on Music/Art at both widths and ART at 390 px, with no page errors; `/drop` loaded without errors but its initial screen has no forward link to inspect.
- Review condition: Revisit if any compact masthead action loses clarity with the thinner rule.

## D101: Make the mastering visuals readable without a panel

- Recorded: 2026-09-27.
- Origin: The user found TONE EQ and LIMITER blank, rejected the solid black panel, the WAVEFORM heading, and a separate playback slider.
- Status: Accepted; implemented in the BITNEEDLE comparison view, with device review pending.
- Decision: Draw the selected EQ response and measured limiter trace on transparent canvases with visible guides. Show a plain status when filters or a limiter trace are absent. Update the processing view as soon as a version is selected. Remove the redundant heading and range input; make the waveform itself the pointer and keyboard seek control.
- Reason: A flat EQ response and zero gain reduction are valid processing results. The plots must communicate those states without suggesting missing data. The waveform already occupies the seek area.
- Alternatives: Keep a dark plotting panel or the separate slider. Both consume space without adding measurements or a distinct action.
- Evidence and verification: Disposable Chromium and WebKit checks showed painted transparent plots and explicit statuses at phone and desktop widths. A keyboard seek check moved the selected player by one second and updated its accessible value. Actual-device visual and listening review remain pending.
- Review condition: Revisit contrast or keyboard step size if a device or accessibility review exposes a problem.
- 2026-09-27 correction: The user wants a separate playhead progress bar, styled as a custom line and thumb. The waveform remains seekable, and the new progress bar takes the accessible keyboard slider role. The plots change only after the audio switch completes, so the displayed processing belongs to the version actually heard. Finishing static cuts join the EQ response; dynamic finishing bands show their planned or measured reductions separately. Older reports with whole-song limiter statistics show mean and peak bars without implying a time trace. An old report with no trace still requires a fresh render for genuine time-resolved limiter activity.
- 2026-09-27 refinement: Match the separate mastering PLAYHEAD band to Track Details' full-height hairline and triangular tab, without crop handles. Its fill indicates elapsed time; the band retains pointer and keyboard seeking. Disposable Chromium and WebKit seeking checks passed before this visual refinement; live appearance review remains pending.
- 2026-09-27 refinement: Use Track Details' circular accent Play button for the mastering Play/Pause control, preserving its 44 px target and current pause state. The user's request follows the matched PLAYHEAD treatment. The previous outlined square looked like a version action rather than a transport control. Live visual review remains pending.
- 2026-09-28 replacement: [D108](#d108-show-measured-frequency-and-limiter-activity-in-the-master-player) replaces the plain EQ line, whole-song limiter bars, and numeric summary labels. The selected EQ settings and measured trace remain valid data.

## D102: Draw master and source audio with one stereo band waveform

- Recorded: 2026-09-27.
- Origin: The user liked the pre-master waveform but found the master waveform blocky, white in places, and surrounded by a dark area.
- Status: Accepted; implemented in BITNEEDLE's saved-WAV visualization, with real-track review pending.
- Decision: Analyze each saved master WAV into 2,048 buckets with separate left and right low, mid, and high mean absolute activity. Cache this as `bitneedle.master-frequency.2` in a new sidecar name, and draw it with the same stereo band renderer used for the source. Keep zoom and seek. Mix band colors by weighted average so a full-spectrum bucket is coloured rather than white. Leave saved audio and mastering renders untouched.
- Reason: Peak-only buckets obscure shape in consistently loud masters. Mean absolute activity exposes passage changes while separate channels retain stereo information. One renderer keeps the comparison visually coherent.
- Alternatives: Reuse the source sidecar for the master, retain the mirrored peak drawing, or change the engine's render output. Each would either misrepresent the master or couple a display change to rendering.
- Evidence and verification: Disposable browser analysis of a synthetic WAV returned two channels and three bands; its canvas had 22,240 painted pixels, no white pixels, and a transparent backing. Live-track and device review remain pending.
- Review condition: Revisit bin count, average quantization, or color contrast if quiet passages disappear or long masters remain hard to inspect.

## D103: Use CUT's pink choice states in DUB selection controls

- Recorded: 2026-09-27.
- Origin: The user asked for CUT's pink button treatment on selected and idle mastering choices, and CMY registration on pink selected rows and chart eligibility.
- Status: Accepted; implemented in BITNEEDLE styles, with live visual review pending.
- Decision: Give CUT format and mastering version choices a pink outline and pink text when idle, then a solid pink face with dark text when selected. Add cyan and yellow offset shadows to pink selected controls and version rows without changing dimensions. Preserve three equal choice slots per row.
- Reason: The controls should share one clear selection language; paint-only shadows avoid row movement during switching.
- Alternatives: Keep neutral idle buttons or add a thicker selected border. The former looks unrelated to CUT; the latter changes layout.
- Evidence and verification: Disposable Chromium checks found the expected colors and shadows for selected and idle CUT, mastering, and chart controls, with unchanged button dimensions. Live visual review remains pending.
- Review condition: Revisit shadow contrast if registration distracts from labels on either theme.

## D104: Store an optional track version label

- Recorded: 2026-09-27.
- Origin: The user requested a VERSION text field below ALBUM in Track Details.
- Status: Accepted; implemented in BITNEEDLE metadata, with live storage review pending.
- Decision: Store an optional `versionLabel` alongside title, artist, and album in each track note. Show and edit it below album, include it in search, and inherit it for derived tracks unless explicitly changed. Keep the existing numeric `version` that identifies derived file revisions.
- Reason: A release version such as a radio edit or club mix is descriptive metadata; it has a different purpose from the internal revision number.
- Alternatives: Reuse the numeric revision field or encode the label in the title. Both obscure one of the two meanings.
- Evidence and verification: A disposable Chromium edit check entered `Club mix` and confirmed the save callback received that label. An isolated OPFS library restored `Radio edit`, saved `Club mix`, then restored that label with numeric revision 2 preserved. Live library review remains pending.
- Review condition: Revisit if distribution metadata later requires structured version types or a controlled vocabulary.

## D105: Resume comparison playback after an audio window underrun

- Recorded: 2026-09-27.
- Origin: The user repeatedly saw a prompt to tap Play after an audio window interruption.
- Status: Accepted; implemented in the BITNEEDLE comparison stream, with real-device playback review pending.
- Decision: Prefetch four bounded PCM windows. If the worklet stops at a missing window, re-send the current window and any pending switch target, then resume automatically at the stopped sample while playback remains requested. Respect a later Pause, Clear, or Close command. Report an error only if refill itself fails.
- Reason: A recoverable file read delay should not require a manual restart or change the listening position.
- Alternatives: Keep the existing three-window prefetch and Play prompt, or buffer whole masters. The first exposes routine buffering to the listener; the second increases Safari memory pressure.
- Evidence and verification: Static review of the worklet protocol found that an underrun set `playing` false and the stream never restarted, even though the frame was retained. Two Node protocol regressions inject an underrun at 12 seconds: one observes the current chunk sent again and playback resumed without an error; the other confirms Pause prevents automatic continuation. Both pass. Real-device verification remains pending.
- Review condition: Revisit prefetch count or worklet signalling if playback still underruns on longer files or slow OPFS reads.

## D106: Open presave details from list data and scope the cover editor

- Recorded: 2026-09-27.
- Origin: The user asked for a separate cover column, immediate presave opening without audience API calls, and a presave cover editor that does not show or load PLANT's label and poster variants.
- Status: Accepted; implemented in BITNEEDLE, with live account review pending.
- Decision: Render a published presave's title, cover and link synchronously from the loaded index. Fetch audience data only when VIEW AUDIENCE is chosen. Open the published cover at `/presaves/<id>/cover` beside the details. Give creation an isolated `presave` draft key and constrain the shared design component to the square cover shape for this flow. Explicit CREATE FROM A PRESSING still derives artwork from the chosen record's saved design.
- Reason: An audience request may be slow or fail while the already available presave details remain useful. A separate creation draft prevents ART's label work from appearing when presave cover is opened. Restricting shapes inside the shared component avoids displaying options the flow cannot use.
- Alternatives: Fetch the audience on every open, reuse the PLANT draft, or render the full shape switcher and redirect unwanted choices. Each adds latency or exposes an unrelated state to the presave flow.
- Evidence and verification: Disposable Chromium checks at 390 and 1,440 px found the detail from one list request with zero new audience calls, the cover at its own address and column, and no LABEL or POSTER chooser on the presave editor. Choosing VIEW AUDIENCE requested the stats and displayed the returned values. The web build passed; live account review remains pending.
- Review condition: Revisit if a direct presave link needs a faster index lookup, or if a future presave format requires a second artwork shape.
- 2026-09-28 correction: The published cover column did not also make `/presaves/create/cover` a new column. The creation DETAILS room now stays in the left column while the shared cover editor opens in the right column at desktop widths. A direct cover URL builds DETAILS from the presave draft; mobile keeps COVER as the only visible column and Back returns to DETAILS. This follows the user's report that the live creation route still opened in one column. Disposable Chromium checks at 390 and 1,440 px confirmed the column positions, direct loading, and preservation of entered names when advancing from DETAILS; live review remains pending.

## D107: Keep record actions in DETAILS, off the mini players

- Recorded: 2026-09-27.
- Origin: The user asked to remove the ellipsis from record mini players while keeping its menu around.
- Status: Accepted; implemented in BITNEEDLE, with live visual review pending.
- Decision: Remove the Music/Art shelf card's action trigger. The same record actions remain on the record's DETAILS page, reached from the player. Keep the reusable record card menu capability for other contexts that request it.
- Reason: The mini player should show the player and artist/heart line without an overlaid utility control; the full record view already provides a stable home for every action.
- Alternatives: Hide the ellipsis with CSS while leaving a focusable button, or remove record actions altogether. The first creates an invisible control; the second removes useful actions.
- Evidence and verification: Code review found the shelf was the sole Music/Art caller of the card menu, and `openDetails` already builds its links from the same `recordActions` list. A disposable Chromium check found zero menu buttons on a mini card and one on a full card that requests it, with no page errors. The web build passed; live visual review remains pending.
- Review condition: Revisit if users need direct shelf access to a frequent record action.

## D108: Show measured frequency and limiter activity in the master player

- Recorded: 2026-09-28.
- Origin: The user found the existing EQ and level plots pointless and asked for a DAW-like display of actual frequencies, waveform, and the places where mastering changed them. They authorized changing the data if needed.
- Status: Accepted; implemented in BITNEEDLE's comparison view, with live-track and device review pending.
- Decision: Replace the plain EQ and level summaries with a frequency view and a limiter waveform view. Analyze each saved original and master WAV into 128 aligned windows of 96 log-spaced FFT bands at 48 kHz, stored in a new `bitneedle.master-frequency.3` browser sidecar beside the existing stereo waveform. At the playhead, draw the measured source and master spectra, shade the measured difference, and overlay the selected static stereo/mid/side filter response and points when active. Draw the saved output waveform with the render's 384-bin measured limiter reduction trace. An older render without that trace displays its waveform and an unavailable state, without inferred gain reduction. This is a display change; the original mix, DSP, outputs, and mastering protocol stay as implemented.
- Reason: A flat filter curve says little about the sound when no EQ was selected. The source/output spectra show actual audio at the same passage, including changes caused by processing other than EQ. The saved limiter trace identifies when the limiter acted, and the waveform gives those moments audible context.
- Alternatives: Continue showing whole-song mean/peak bars or derive a limiter trace from waveform differences. The bars hide timing; waveform differences include other processors and cannot identify limiter gain. Live AudioContext analysis would go blank while paused and would require another audio path.
- Evidence and verification: The interface references are a DAW vendor's guides to its channel EQ analyzer and its limiter. The channel EQ guide describes an FFT spectrum overlaid with EQ curves, and the limiter guide describes a reduction meter. Those are interface references, not DSP rules. A disposable synthetic-WAV worker check returned the 1 kHz tone near 992 Hz and a nonempty waveform; isolated 80 Hz through 15 kHz tones landed in the expected log bands. Disposable Chromium checks painted spectra, filter points, waveform, and reduction regions in DUB and dark themes without page errors. Real-track and live-device review remain pending.
- Review condition: Revisit spectral resolution or color contrast if bass detail, quiet passages, or selected-version changes are hard to read on actual masters. Add per-band time traces to the render only if listening review shows a need to distinguish dynamic EQ action from the measured total spectral change.

## D109: Human-level assessment and peak-control program

- Recorded: 2026-09-28.
- Origin: The user asked for an assessment of the approach against human mastering and asked to proceed toward human-level outputs.
- Status: Accepted; peak control implemented in D110. Broad tone, width, selection and evaluation remain open.
- Decision: Record the assessment in the human-level assessment. Rebuild peak control first: a two-stage program-dependent limiter with hold and raised-cosine lookahead, a ceiling-relative oversampled clipper with a bounded budget, and a held true-peak ceiling. Broad tone, width, selection policy, and evaluation follow in that order.
- Reason: The production set path applies one limiter and near-inaudible EQ. The human track-e master differs by 1 to 2 dB in broad bands, 3.5 to 4.5 dB in side/mid, 0.9 LU in loudness, 1 dB in true peak, and 1.4 dB in short-term gain variation. Peak control is the largest measured difference and is self-contained in the renderer.
- Alternatives: Continue calibrating gated bells by listening, or fit the human reference curve to every track. The first keeps the effective change near 0.3 dB; the second was rejected in D063 because the two human references moved the low mids in opposite directions.
- Evidence: the reference study, the loudness-objective and core-foundation run records, the track-a set report.
- Limits: Listening evidence is from one listener without recorded playback mode. One full human master exists.
- Verification: Corpus renders at fixed delivered loudness with attack loss, crest loss, limiter and clipper reduction, and short-term gain variation recorded before and after each step.
- Review condition: Revisit the order if listening on the new peak control shows tone or width as the larger remaining difference.
- Work reference: TODO.md, "Human-level program".

### Updates

- 2026-09-28: Phase 1 complete (D110). The user listens only to complete masters, so listening review moves to phase 6 after the complete chain in phase 5. The phase plan with stage order, bounds and gates is in the human-level assessment.

## D110: Peak control with hold, lookahead, and ceiling-relative clipping

- Recorded: 2026-09-28.
- Origin: D109 program step 1. The single-stage limiter carried every peak on the production set path with a 1 ms exponential attack, no hold and 0.45 dB of unused ceiling margin.
- Status: Accepted; implemented in `src/dsp.rs`, `src/render_blocks.rs`, `src/treatment_render.rs` and `src/set.rs`. Listening review is open.
- Decision: Replace the limiter with one streaming state machine: fast release in the dB domain, a 12 ms hold that applies when another over-ceiling run ended within 12 ms, a 3 ms trailing maximum and three cascaded 1 ms moving averages. Add a ceiling-relative quadratic-knee soft clipper after the limiter. Set versions deliver at −0.5 dBTP with the limiter bounded 1.5 dB (base) or 2.5 dB (denser) above the ceiling (`set.peak-share.v1`). The internal ceiling margin is 0.1 dB and the final trim goes to the requested ceiling. A sustained release stage exists as a setting and is off in every production chain. Remove the old pipeline's adaptive-release finishing family. The loudness search stops for a reversal above 0.1 LU instead of the target tolerance.
- Reason: On four corpus mixes at their set targets, maximum limiter reduction fell by 1.5 to 5.4 dB, mean reduction fell on seven of eight renders, P90 attack loss fell by 0.2 to 1.9 dB on base targets and short-term gain span fell on six of eight. The hold removes limiter release distortion on sustained low frequencies: a 40 Hz tone 6 dB over the ceiling measures −30 dB THD without hold and none with 12 ms hold. The sustained stage measured 0.7 to 0.9 dB more limiter work and a wider gain span for no attack gain, so it is off.
- Alternatives: Keep the sustained stage at 200 to 400 ms (measured cost, and it made loudness nonmonotonic in drive); hold on every maximum (costs level on isolated clicks); clip before the limiter (cannot bound clip depth); keep the −1 dBTP ceiling with 0.45 dB of margin (gives away level against every reference).
- Evidence: the peak-control run record, its renders and the limiter-recovery and peak-control probes.
- Rule and code references: `set.peak-share.v1`, `dsp::Limiter`, `dsp::SoftClip`, `render_blocks::render`, the peak-control design.
- Limits and counterexamples: P90 50 ms crest loss rises by 0.1 to 0.6 dB because the clipper removes peaks by design. Repeated full-scale clicks recur inside the hold window and cost held reduction; the old-pipeline fixture now lands 0.44 LU under target at its drive bound. The denser target of the track-f premix needs 16.8 dB drive and 15 dB maximum reduction; the density rule, not the limiter, sets that target. Clip budgets and hold are provisional until listening.
- Verification: `cargo test --release --locked`, clippy and fmt pass. Block renders match whole-buffer renders bit-for-bit across five block sizes and three clip budgets. All 38 corpus probe renders match their targets. Listening is open.
- Review condition: Revisit the clip budgets, hold and ceiling after listening to the corpus at delivered level, and when a human master of another corpus track becomes available.
- Work reference: TODO.md, "Human-level program".

## D111: Broad tone, width, glue and candidate selection in the set

- Recorded: 2026-09-28.
- Origin: D109 phases 2 to 5. The set path applied near-inaudible EQ, no width change, no low-end control and no compressor, and its selection preferred the smallest passing change.
- Status: Accepted; implemented in `src/broad_tone.rs`, `src/width.rs`, `src/selection.rs`, `src/set.rs` and `src/distributed.rs`. Listening review is open.
- Decision: Add broad tonal families (`open`, `full`, `combined`) from the long-term source spectrum against corpus interquartile bounds, capped at ±2 dB per move and 3 dB per family. Add a side high-pass at 100 Hz when side power below 100 Hz is at least 2% of the total there. Try +3 and +1.5 dB wide side bells in `open` and `combined`. Derive denser versions from the chosen base candidate with a glue trial and a 10 dB limiter bound. Render every version as a candidate ladder, strongest first, keeping the first candidate that passes its gates. Replace the −40 dB residual duplicate screen with the description-based signature screen. The worker plan schema is `mastering.worker-plan.2`; the renderer keeps two prepared prefixes.
- Reason: The human track-e master moved low mid, presence and slope toward the corpus middle, and widened by 3.5 to 4.5 dB side/mid. Source-relative bounds propose the same direction on track-e without a universal curve. A ladder chooses the largest passing move, which is how a human master is selected.
- Alternatives: Fit the human reference curve (rejected in D063); keep gated 1.5 dB bells (near-inaudible); keep the residual duplicate screen (it excluded a preferred version in D065).
- Evidence: the broad-tone and complete-master run records and their renders.
- Rule and code references: `set.broad-tone.v1`, `set.low-end-mono.v1`, `set.side-lift.v1`, `set.glue.v1`, `set.dense-bound.v1`, `set.candidate-ladder.v1`, `set.described-duplicate.v1`.
- Limits and counterexamples: The bounds come from 19 related mixes and one human master. A synthetic test signal proposes families that fail every gate, which is the intended no-opportunity path. Cloud workers built from an older engine reject the version 2 plan until rebuilt.
- Verification: `cargo test --release --locked`, clippy and fmt pass. Corpus renders are recorded in the run documents. Listening is open.
- Review condition: Revisit every bound after the phase 6 listening review.
- Work reference: TODO.md, "Human-level program".

### Updates

- 2026-09-28: The first complete track-e render delivered 12 versions in 9.4 minutes and track-h 6 in 6.7 minutes. The user judged 12 too many and asked for three or four. The set now has fixed roles under `set.master.v1`: Essential, Master (joint broad tone, strongest stem focus, side-lift ladder), Denser Master and Master · Narrower, with Denser Essential as the fallback. The `open`, `full`, `combined` and separate focus families are gone. Loudness searches of later candidates warm-start from the Essential drive, the D066 comparison-only fallback is no longer rendered, and the glue trial is off. The track-e excerpt renders four versions in 26 seconds.

## D112: Broad tone from the recording, not a corpus

- Recorded: 2026-09-29.
- Origin: The user rejected fitting the engine to a corpus. D111's broad-tone bounds were the corpus interquartile range of 19 mixes, and a mix at the corpus middle received no move.
- Status: Accepted; implemented in `src/broad_tone.rs` as `set.broad-tone.v2`.
- Decision: Derive every broad tonal move from the recording. ISO 226 equal-loudness contours at the delivered level change give a bass and an air shelf that keep the mix's perceived balance when the master plays louder. The vocal's power share of the arrangement per region, from aligned stems, gives a presence lift and a low-mid cut for half the vocal's share deficit against its body. Remove the corpus bounds, the tilt move and the `Bounds` structure. Mix-and-master pairs remain validation of where an expert lands, never a target.
- Reason: Both rules exist for every recording and change with it. On the 30-second track-e excerpt the rules propose bass −1.6 dB and low mid −1.9 dB; Master moves low mid −1.4, presence +0.8, air −0.6 and side/mid +1.8 dB, the same directions as the human master (−2.1, +1.3, −2.3, +3.0) with no reference in the loop. On the track-d excerpt, which received no move under the corpus bounds, the rules propose bass −1.2 dB and low mid −0.9 dB.
- Alternatives: Keep corpus bounds (fits this EP); an outcome prior from masters (fits a population); a user-supplied reference per project (fits one record). All rejected as fitting.
- Evidence: the broad-tone run record; the track-e and track-d excerpt sets.
- Limits and counterexamples: The 75 phon monitoring assumption is a stated constant. Masking moves need stems and vocal activity; an instrumental receives equal-loudness moves only. Half the deficit is a choice pending listening.
- Verification: Unit test on the contour direction; set tests; both excerpts deliver four audited versions in about 25 s each. Listening is open.
- Review condition: Revisit the phon assumption and the half-deficit scale after listening; add resonance, harshness and sibilance detectors as further recording-derived rules.
- Work reference: TODO.md, "Human-level program".

## D113: Bolder general moves and a Bold version

- Recorded: 2026-09-29.
- Origin: The user cannot judge single tracks by ear with confidence and asked for best guesses informed by acoustics and research, applied generally, with bolder choices because a set holds up to four versions.
- Status: Accepted; implemented in `src/broad_tone.rs` and `src/set.rs`.
- Decision: The masking moves restore the vocal's share to parity with its body (the full deficit, capped at 2 dB) instead of half. The fourth version is Master · Bold: the chosen Master tone scaled by 1.5, side lift excepted, gated for mono compatibility only. Master · Narrower is removed. The Master ladder gains focus-free fallbacks after the scaled ones, and scaled fallbacks scale their tonal gate minimums. The 75 phon monitoring assumption stays: the contour compensation for a 4 LU rise differs by under 0.05 dB between 65 and 90 phon.
- Reason: Parity of share across regions is the natural target of a masking rule; half was a hedge for listening that will not arrive per track. A Bold version lets the listener choose between parity and past it without the engine guessing taste. On the full track-a the body-region stem focus and the presence move opposed each other in the gates and no Master delivered; focus-free fallbacks resolve that generally.
- Alternatives: Keep half the deficit and wait for listening; keep Narrower as the fourth version. Both rejected as hedges.
- Evidence: the track-a, track-e and track-d excerpts: every version delivers, Master passes at its strongest candidate, Bold passes its mono gates on all three.
- Limits and counterexamples: The equal-loudness compensation cuts the sub where the human track-e master kept it; it is physics, and the divergence is recorded in the complete-master run record. Bold is 1.5 times a general rule, not a measured optimum.
- Verification: full suite passes; three excerpts render four versions in under 30 s each.
- Review condition: Revisit the factor and the parity target when several mix-and-master pairs show a consistent gap.
- Work reference: TODO.md, "Human-level program".

### Updates

- 2026-09-29: Bold scales only the stereo (broad-tone) bands. On track-g the Master tone carried two 4.4 dB focus bells; scaled by 1.5 and clamped to 6 dB, the summed mid response exceeded the 9 dB domain bound and `bold_candidate` returned no candidate, so the set delivered three versions with a bare `no-opportunity` event. Foundation and focus bells are measured optima and stay as measured.

## D114: Lead-role masking

- Recorded: 2026-09-29.
- Origin: The masking rule of D112 used the vocal's share of the arrangement, so an instrumental received only the equal-loudness moves. The user wants no genre or user input; the lead must come from the recording.
- Status: Accepted; implemented in `src/stem_evidence.rs` and `src/broad_tone.rs` as `set.broad-tone.v3`.
- Decision: Each role's active frames and its share of total estimated power over those frames are measured per one-third-octave band. The vocal leads by precedence when active for at least 10 s and 15% of the frames where any role is active. Otherwise the role with the highest mean share of 500 Hz–5 kHz leads when it is not drums, holds at least a 35% share, beats the runner-up by 1.5 dB, is active for 10 s and has a log-frequency energy centroid of at least 500 Hz over 250 Hz–5 kHz. The lead's regions scale with its centroid c, clamped 500–2000 Hz: low mid [c/4, c/2), body [c/2, 2c), presence [2c, 5c), with bells at 0.35c and 3.2c. The vocal keeps c = 1 kHz, which reproduces the D112 regions, bells and gates exactly.
- Reason: The parity premise, that a lead's share of the arrangement should not fall from its body to its presence, holds for any lead whose energy centres in the midrange. Drums are excluded because a kit's share is high in presence and low in body by construction; a bass lead below 500 Hz has no presence to restore.
- Alternatives: Ask for genre or lead instrument (rejected by the user); vocal only (leaves instrumentals without masking moves).
- Evidence: the broad-tone run record; the three stem excerpts render identically to D113 for their vocal leads.
- Limits and counterexamples: A sparse arrangement yields shares near one and no deficit, so no move. A two-lead arrangement takes the stronger share. No instrumental with stems exists in the corpus yet.
- Verification: suite passes; excerpt renders unchanged for vocal-led material.
- Review condition: Revisit the lead thresholds on the first instrumental corpus with stems.
- Work reference: TODO.md, "Human-level program".

## D115: Density targets for wide-range material and bounded Denser versions

- Recorded: 2026-09-29.
- Origin: The base target ignored passage-to-passage contrast and floored at −16 LUFS; the Denser target was bounded only by a 10 dB limiter maximum. Both carry a pop assumption.
- Status: Accepted; implemented in `src/set.rs` and `src/selection.rs` as `set.source-density.v3` and `set.dense-bound.v2`.
- Decision: Base target = −(50 ms crest median + half the 12 s window contrast above 6 dB), bounded −20 to −8 LUFS. Denser targets beyond the base render's limiter headroom (8 dB minus the base's measured maximum reduction) are not tried, and every dense candidate is gated: maximum reduction ≤ 8 dB, mean reduction ≤ 3 dB, P90 attack loss ≤ 2 dB more than the base render. A family without a passing dense candidate delivers no Denser version.
- Reason: Peaks of the loudest passages sit near half the contrast above the integrated level, so wide-range material needs the lower target to keep them under the ceiling. AES TD1008 recommends −16 to −20 LUFS for streaming distribution of wide-range programme. Streaming services normalise near −14 LUFS, so a Denser version buys no playback loudness and is a character choice that must be bounded by punch loss; typical mastering limiting is 2 to 4 dB mean and up to 6 dB peak.
- Alternatives: Keep the −16 floor and the 10 dB bound (accepts hypercompression on dynamic material); drop Denser altogether (removes a version the user liked on track-e).
- Evidence: a design spike; pop corpus targets unchanged because their window contrast is under 6 dB.
- Limits and counterexamples: No classical or sparse acoustic recording is in the corpus; the −20 floor and the contrast term are untested on such material.
- Verification: suite passes; excerpt Denser versions still deliver within the bounds.
- Review condition: Revisit the bounds on the first wide-range recording.
- Work reference: TODO.md, "Human-level program".

## D116: Equal-loudness compensation at the normalised playback level

- Recorded: 2026-09-29.
- Origin: On all eight rendered confirmation mixes the engine cut bass 1.3 to 2.6 dB through the equal-loudness rule while every reference master added bass. The rule assumed the listener hears the master 3 to 5 LU louder than the mix.
- Status: Accepted; implemented in `src/set.rs`.
- Decision: The delivered gain that drives the ISO 226 compensation is the smaller of the base target and −14 LUFS, minus the source loudness. Streaming services normalise playback near −14 LUFS (one service at −16), so a master above that level is not heard louder than its mix by the difference in integrated loudness. For sources near −14 LUFS the compensation vanishes; for a source above −14 it becomes a small bass lift. The Denser headroom estimate also uses the measured 1.5 dB of maximum limiter reduction per LU of target.
- Reason: The physics of the contours is unchanged; the level assumption was wrong for how masters are heard. The one systematic disagreement with every reference is explained by that assumption.
- Alternatives: Keep the raw delivered gain (contradicts every reference and streaming practice); drop the rule (loses the correct compensation for listeners who play louder than the mix).
- Evidence: the confirmation comparison; the design agent's render evidence in a private record.
- Limits and counterexamples: Playback outside normalised services, such as a DJ system, hears the full gain; the Denser version keeps that case within its bounds.
- Verification: suite passes; the three stem excerpts render with the bass shelf reduced to zero for sources within 0.5 LU of the normalised level.
- Review condition: Revisit if a reference master made for a non-normalised context shows the opposite.
- Work reference: TODO.md, "Human-level program".

## D117: Peak-control tone restoration

- Recorded: 2026-09-29.
- Origin: The decision review of the request for richer, recording-driven mastering decisions. Peak control alone changed the matched-level balance of seven of eight confirmation mixes by 0.3 to 1.2 dB per region.
- Status: Accepted; implemented in `src/treatment_render.rs` and `src/set.rs` as `set.peak-tone-restoration.v1` and `set.dense-bound.v3`. The worker plan schema is `mastering.worker-plan.3`.
- Decision: Every set version renders once, measures the change from its processed prefix to its output in the six stereo bands at matched loudness, and renders again with one broad filter per material band that cancels the change, bounded at ±1.5 dB. Changes under 0.2 dB are held. A denser target beyond the base render's limiter headroom falls to the largest target within it instead of being dropped.
- Reason: The limiter lowers the gain most at the kick and bass peaks, so the low end loses more long-term power than sustained midrange content. At +5.7 LU the change reached −1.24 dB sub (track-c), +0.94 dB body (track-a) and −0.67 dB upper (track-f). The change is opposite to the human and AI reference masters, which raise the sub and lower the low mids. A mastering engineer restores what the limiter took. The measurement comes from each version's own render, so Denser versions receive a larger restoration than Master, and a source with little limiting receives none.
- Alternatives: Predict the change from the Essential render (the Denser change is 1.3 to 2 times the base change and does not scale with mean reduction); restore inside the limiter with band-split gain (a different processor, larger change); leave the change (keeps a systematic tonal side effect).
- Evidence: the peak-tone probe on eight full confirmation mixes; excerpt sets against the 8e183e8 binary. On the track-a excerpt the largest band change fell from 0.56 to 0.11 dB on Master and Essential presence against body moved from −0.92 to −0.15 dB. On the track-d excerpt Denser Master sub against bass is +1.02 dB against +0.78 dB on Master. track-e is unchanged: its limiter mean reduction is 0.07 dB.
- Rule and code references: `set.peak-tone-restoration.v1`, `set.dense-bound.v3`, `treatment_render::Restoration`, `treatment_render::restoring_filters`, `set::dense_candidates`.
- Limits and counterexamples: Restored bass raises limiter work, so 20 to 25% of a large low-end loss remains. On the track-a excerpt maximum reduction rose by 0.4 to 0.5 dB; the Denser target fell from −8.79 to −8.89 LUFS under the headroom rule. Bands under 1% of the prefix power are not restored; the 12 kHz band on track-a keeps −0.4 to −0.7 dB. Versions that need restoration render one extra loudness search: 12 to 24 probes on the track-a excerpt. Gates compare restored renders with the restored Essential.
- Verification: suite, clippy and fmt pass; unit test on the filter solve; three excerpt sets deliver the same version roles as before. Listening is open.
- Review condition: Revisit the 0.2 dB threshold and ±1.5 dB bound after the confirmation re-render; revisit band-split peak control if the residual low-end loss stays above 0.3 dB.
- Work reference: TODO.md, "Human-level program".

## D118: Mastering review of peak control, restoration and stereo moves

- Recorded: 2026-09-29.
- Origin: The user asked for a review of the engine as a mastering engineer would do it, to find the changes that give a dynamic engine with professional results.
- Status: Proposed. No rule changed.
- Decision: Put three peak-control items before the D117 confirmation render: a local restoration measure, split-band peak control, and a new measurement of the limiter release and clipper knee. Then do dynamic tone from the stems, stereo bounds, Denser gates on contrast and attack loss, and the remaining plan items. The order is in the engine review and the human-level plan.
- Reason: On eight full mixes at +5.7 LU, the kick and bass set 42 to 78% of the required limiter reduction. The 2 to 10 kHz band is 1 dB or more down in 13 to 32% of 10.7 ms frames, with correlation −0.40 to −0.65 against the low-band level. The whole-song measure of D117 includes the level change between passages, 1.2 to 4.2 dB P10 to P90. Its mean sub change is −0.43 dB against −0.09 dB inside passages.
- Alternatives: Keep D117 and render the confirmation mixes first (the render would measure a restoration that corrects passage levels); raise the limiter release alone (longer ducking of the upper band); lower the delivered loudness (the user sets loudness from the source crest, D115).
- Evidence: the limiter-character probe on the eight D117 mixes at +4.2 LU with a 1.5 dB clip budget and +5.7 LU with 2.5 dB; the confirmation comparison for attack contrast and width; a subsonic check of the same mixes.
- Limits and counterexamples: The probe renders peak control without tone. The 1.5 dB and 2.5 dB budgets and the 50 ms release are fixed in the probe. The low-band share uses the larger band at the frame of each over; it does not model the summed peak. A high-pass at 20 to 30 Hz raises sample peaks by 0.4 to 2.0 dB, so the review keeps the chain without a subsonic filter.
- Verification: The findings are measurements; listening is open.
- Review condition: Record a decision for each item as it is implemented. Revisit the order if the local restoration measure changes the low-end findings.
- Work reference: TODO.md, "Human-level program".

### Updates

- 2026-09-29: After D121 the order changes. Compression by measured need (item 9), width from the source (item 6 with plan A5), and stem-derived dynamic tone (plan A4) come before the limiter release and clipper knee (item 3). The set path has no active compressor and a fixed side-lift ladder, so these items add processing that follows the recording. The human-level plan holds the order.


## D119: Four-panel detail view in the MASTER screen

- Recorded: 2026-09-29.
- Origin: Workstream C of the human-level plan. The user asked to finish the interface.
- Status: Accepted; implemented in the BITNEEDLE web app (`web/js/mastering-processing.mjs`, `web/styles/dubplate.css`). The native views keep the D108 panels.
- Decision: Replace the D108 FREQUENCY and LIMITER panels with four tabs: EQ, CHANGE, DYNAMICS and LIMITER. EQ draws every applied filter with numbered nodes, a filter table and the measured spectra at the playhead. CHANGE draws the matched-level change per band over time. DYNAMICS draws loudness and crest per slice. LIMITER draws the driven input peaks, output peaks, gain reduction and clip marks from `mastering.limiter-trace.2`. Each scheme has its own series colors, and text uses the ink and muted colors.
- Reason: An engineer reads a master in the form of channel EQ, analyzer, compressor and limiter plug-ins. Each curve is an engine setting or a measurement of the saved audio. The matched-level offset needs the original's loudness in each row.
- Alternatives: Keep the two D108 panels (no time view of the band change or the peak levels); compute the views in the renderer (adds work to every render).
- Evidence and verification: Disposable Chromium and WebKit checks on the track-a excerpt set, at 432 and 375 px, in DUB, Light and Dark. There were no page errors and no horizontal overflow. The parameters read −13.2 → −10.0 LUFS and +3.2 LU. The worker analyzes two 30 s WAVs in 0.6 to 0.8 s. The palette passes the color-vision separation and contrast checks in each scheme, with dashes and node numbers as secondary encoding. The module is 47 KB against the planned 30 KB.
- Limits: The native mastering views accept only `mastering.limiter-trace.1`. Stem-derived envelopes from plan item A4 are not drawn yet.
- Review condition: Revisit the slice count if full-song CHANGE maps are too coarse, and port the tabs to the native views under the Swift alignment plan.
- Supersedes: The display part of D108.

## D120: Restoration measured inside passages, with continuous correction

- Recorded: 2026-09-29.
- Origin: Item 1 of the engine review (D118).
- Status: Accepted; implemented in `src/treatment_render.rs` and `src/dsp.rs` as `set.peak-tone-restoration.v2`.
- Decision: Measure the tone change that peak control causes inside 0.34 s windows (four 4096-sample blocks), at the matched K-weighted level of each window. Weight each window by the band's prefix power and skip windows more than 40 dB under the loudest. Shrink each change by 0.1 dB before it is restored, so the correction is continuous in the change. A band is material when it holds 1% of the prefix's K-weighted power.
- Reason: The whole-song measure of D117 read the level change between passages as a tone change. On eight full mixes at +5.7 LU the mean change inside passages is −0.12 dB below 120 Hz, +0.17 dB at 500 Hz to 2 kHz and −0.28 dB at 2 to 6 kHz. The whole-song measure had read a mean sub change of −0.43 dB. On track-a the restoration now cuts the bass shelf by 0.54 dB and lifts 3.46 kHz by 1.11 dB, where D117 lifted the bass. The hard 0.2 dB threshold of D117 gave the Essential and Master of the track-d excerpt different corrections for body changes of +0.19 and +0.21 dB. That difference failed the Master tone gate (0.29 to 0.36 dB against 0.52 dB), and the set fell back to Denser Essential. With the continuous correction both versions get the same filters within 0.02 dB, and the set is Essential, Master and Denser Master again. The raw power share left 6 to 12 kHz out on most mixes (0.42 to 0.86%); its K-weighted share is 1.39 to 2.96%.
- Alternatives: Keep the whole-song measure (restores passage level changes as tone); render a third time to remove the residual (one more render for each restored version); lower the raw power threshold (admits bands by raw power, which the bass dominates).
- Evidence: the limiter-character probe with `MASTER_PROBE_RESTORE` on the eight D117 mixes; excerpt sets for the hard threshold, the track-d continuous correction and the final rule.
- Limits and counterexamples: The full-song probe ran before the dead zone and the K-weighted material test. There, the largest residual in a material band after restoration was 0.09 to 0.24 dB per mix. On the final excerpt sets the residual is 0.1 to 0.27 dB. The limiter takes back part of a restoring boost: 0.24 dB of bass stays on track-d, and 0.27 dB of presence stays on track-a Denser Master. The 12 to 20 kHz band stays unrestored (K-weighted share 0.22 to 0.84%).
- Verification: Five unit tests cover passage level changes, tone inside passages, K-weighted material and continuity. The suite, clippy and fmt pass. The final excerpt sets keep the D117 version roles.
- Review condition: Measure again after split-band peak control (review item 2), which changes how the limiter reacts to the restoring filters.
- Supersedes: The measurement and the threshold of D117. D117's filter set, bound and second render stay.
- Work reference: TODO.md, "Human-level program".

### Updates

- 2026-09-29: The adversarial review found two gaps. The window level came from a rectangular K-weighted sum while the band powers came from windowed blocks. The level now comes from the same windowed blocks. FFT rounding differs between native and WASM builds, so the measured changes are rounded to 0.0001 dB and the material shares to 1e-6. Two unit tests now cover the power weighting and the quiet-window rule, and an integration test covers the render wiring.

## D121: Split-band peak control rejected at the fixed-drive measurement

- Recorded: 2026-09-29.
- Origin: Item 2 of the engine review (D118); step 1 of the split-band design plan.
- Status: Rejected. The engine is unchanged. `set.split-band.v1` is not implemented.
- Decision: Do not add the low-first split-band gain computer. The D110 wideband limiter and the clipper stay the only peak control. The next peak control item is the limiter release and clipper knee (review item 3).
- Reason: The plan continues only when the wideband mean reduction falls by 30% or more and the upper-band P05 magnitude falls by 25% or more on 3 of the 4 kick-led mixes. At +5.7 LU, 0 of 4 mixes meet both conditions. At +4.2 LU, 1 of 4 meets them. The wideband mean fell by 15 to 28% at +5.7 LU on the kick-led mixes, where kick and bass carry 62 to 77% of the required reduction at the over frames. The stage requests a cut on 0.1 to 1.9% of frames. The wideband limiter reduces the gain on 50 to 86% of frames with the stage and 58 to 89% without it, because its release follows each remaining over. The pre-peak arm gives the same result within 0.05 dB.
- Alternatives and tradeoffs: A higher crossover removes more wideband reduction. The removed share rises at every grid step up to 252 Hz on all four kick-led mixes, so the plan's crossover check selects 252 Hz on all four. At 200 Hz the step-1 conditions hold on 3 of 4 mixes and at 252 Hz on 4 of 4. At 158.7 Hz and above, the low-band P05 falls by 2.05 to 2.65 dB on track-b and track-d, over the plan's 2.0 dB limit, and the bass band loses 0.1 to 1.1 dB inside passages. At each crossover, 2 or fewer kick-led mixes meet both step 1 and the pumping limit. The low-band all-pass rotation is rejected in the peak-control probe run record.
- Evidence: the split-band run record; its raw results. The limiter-character probe with `MASTER_PROBE_SPLIT_IDEAL=1` on the eight D117 confirmation mixes, one at a time, at +5.7 LU with a 2.5 dB clip budget and +4.2 LU with 1.5 dB, and a crossover grid from 63 to 252 Hz at +5.7 LU.
- Rule and code references: The probe path of the limiter-character probe. The `measure` arm reproduces the engine render bit for bit on all 16 runs.
- Limits and counterexamples: The drive is fixed at the drive of the D110 render. At that drive the split output is 0.01 to 0.26 LU quieter, so a loudness-matched render would need more drive. The upper-band P05 improved on all 16 runs, by 0.07 to 1.38 dB, and by 24 to 33% on the kick-led mixes. The cost was 0.9 to 1.4 dB of sub level inside passages and 0.6 to 1.8 dB lower low-band P05 on the kick-led mixes. The crossover and timing come from the plan's table, which the plan derived from the saved stem evidence.
- Verification: The suite, clippy and fmt pass. No listening test.
- Review condition: Measure again when the limiter release changes (review item 3). A release that stops following each over changes the share of the wideband mean that the over frames cause.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D122: New engine plan from the audit, the code review and the engineer research

- Recorded: 2026-09-29.
- Origin: The user asked for a new look at the whole engine and a new plan.
- Status: Accepted. A separate plan record holds the plan.
- Decision: Build the chain as input condition, tone, compression, de-esser, peak control and loudness, with each setting from the recording and the stems as guides. A stage stays only when it changes the mix by 0.5 dB or more where it applies. Compression comes first.
- Reason: The saved reports of the 12 confirmation mixes show that loudness, the Master broad tone and the side lift make almost all of the audible change. The Essential bells change the mix by under 0.3 dB on most mixes. No delivered version has a compressor or a de-esser. The engineers in the research notes compress by need with slow attack and a high-passed detector, clip before the final limiter, lower side high frequencies, and stop loudness where the sound degrades. The engine lifts side high frequencies, clips after the limiter, and sets loudness from the source crest.
- Alternatives and tradeoffs: Continue the D118 order (sub-audible peak-control items first); fit the engine to reference masters (one human master exists, and D112 requires settings from the recording).
- Evidence: The audit and review summaries in the mastering plan; the split-band run record; the engineer research notes of the app repository; the tutorial review.
- Limits and counterexamples: The saved confirmation reports predate D116, D117 and D120. The raw interview transcripts are deleted; the notes hold few numbers.
- Verification: Each step has a probe on the eight confirmation mixes before any engine change.
- Review condition: After step 3 of the plan, render the confirmation mixes and compare with the reference masters and an online mastering service.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: The order in D118 and its update.

## D123: Bus compression before peak control: first measurement

- Recorded: 2026-09-29.
- Origin: Step 1 of the mastering plan (D122).
- Status: Superseded by the update below. The engine is unchanged.
- Decision: Do not add a linked compressor before peak control in the set versions. The next step is the peak control order.
- Reason: At matched loudness (+5.7 LU) on the eight confirmation mixes, the criteria set before the run fail in all three arms. The limiter mean falls by 15% or more on 0 of 8 mixes (at most 14% with 1 dB of mean compression). The 2 to 10 kHz gain P05 becomes worse on 8 of 8 mixes, by up to 0.52 dB, because the compressor adds its own gain change. With 1 dB of mean compression the window contrast loss rises by more than 0.5 dB on 5 mixes. The compression lowers the P90 attack loss on every mix, by 0.08 to 0.56 dB.
- Alternatives and tradeoffs: The D111 glue settings (0.23 to 0.50 dB mean reduction) and stem-timed settings with a detector high-pass (0.03 to 0.31 dB) change the limiter by 11% or less. A deeper compressor loses more window contrast. The 50 ms crest of the reference master of track-e (−1.20 dB against −1.84 dB for the D110 chain at +5.7 LU) does not come from compression in these measurements: the compressor arms change the 50 ms crest by 0.1 dB or less.
- Evidence: The compression probe run record; its result files.
- Rule and code references: `MASTER_PROBE_COMPRESS` in the limiter character probe; the probe comparison script.
- Limits and counterexamples: One loudness (+5.7 LU) and one limiter release (50 ms). The sources have 13.4 to 17.0 dB crest; a mix with wider section contrast can respond differently. The upper-band P05 counts any broadband gain change, so it counts compression as ducking.
- Verification: Probe only. No listening test.
- Review condition: Measure again after the peak control order changes and after the loudness step, and on a mix with wide section contrast.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

### Updates

- 2026-09-29: The rejection is withdrawn. Two of the five criteria counted the intended effects of a compressor as damage: the upper-band P05 counts the compressor's own gain change, and the window contrast loss counts the smaller level difference between loud and quiet passages. The run shows that compression does not reduce the limiter work or the limiter pumping, and that it lowers the P90 attack loss on every mix (0.08 to 0.56 dB with 1 dB of mean reduction). The next probe finds, for each mix, the deepest mean reduction (0.5 to 2 dB) that lowers the attack loss and keeps the added window contrast loss at 0.5 dB or less.


## D124: Compression before peak control where the recording keeps its section contrast

- Recorded: 2026-09-29.
- Origin: Step 1 of the mastering plan; D123 update.
- Status: Provisional. Implemented in `src/compressor.rs` (`mastering`) and `src/set.rs` as `set.compression.v1`.
- Decision: Every set version gets a linked 2:1 compressor with a 6 dB knee before peak control. The attack comes from the drum decay P50 (10 to 50 ms), the release from the drum interval P50 (100 to 600 ms) and the detector high-pass from the bass body frequency (40 to 150 Hz). The threshold gives the deepest mean reduction from 2 dB down to 0.5 dB, in 0.25 dB steps, at which the compressor alone removes 0.5 dB or less of the contrast between the active 12 s windows. No depth passes: no compressor.
- Reason: At matched loudness the stem-timed compressor lowers the P90 attack loss on every mix where a depth passes. The rule chooses 2 dB on track-e and track-f and 1.5 dB on track-g, within 0.5 dB of the deepest passing depth of the rendered sweep on all eight mixes.
- Alternatives and tradeoffs: A fixed depth flattens the section contrast by up to 1.9 dB (track-d at 2 dB). A choice from renders costs four renders per recording. A threshold that follows the section level is the next probe.
- Evidence: The compression probe run record, depth sweep; its result files.
- Rule and code references: `compressor::mastering`, `compressor::Timing`, `set::Report.compression`; tests in `tests/compressor.rs`.
- Limits and counterexamples: Goal check 1 of the plan fails: 3 of 8 mixes get compression. On track-h, track-b and track-d the lightest depth adds 0.53 to 0.56 dB of contrast loss. The rule is stricter than the rendered chain on track-a and track-c. One loudness (+5.7 LU) measured.
- Verification: The suite, clippy and fmt pass. No listening test.
- Review condition: Replace the fixed threshold with one that follows the section level if that probe meets the goal checks.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: The glue trial of D111 (`GLUE_TRIAL` stays false).

## D125: Compression threshold that follows the section level

- Recorded: 2026-09-29.
- Origin: Goal check 1 of the compression step failed under D124.
- Status: Superseded by D202. Implemented in `src/compressor.rs` as `set.compression.v2`; `Settings.section_ms`.
- Decision: The compressor threshold is an offset above the section level, the detector power averaged over a centred 4 s window. The offset gives the deepest mean reduction from 1.5 dB down to 0.5 dB at which the compressor alone removes 0.5 dB or less of window contrast. The attack, release and detector high-pass stay as in D124.
- Reason: A fixed threshold reduces loud passages more than quiet ones and removed 0.51 to 1.92 dB of section contrast at 2 dB. With the section-relative threshold the added contrast loss is −0.28 to +0.09 dB, so every mix gets compression. In the engine render the rule applies 1.5 dB on all eight mixes and lowers the P90 attack loss on all eight, by 0.12 to 0.61 dB. The maximum is 1.5 dB because the attack gain stops rising there.
- Alternatives and tradeoffs: The fixed threshold of D124 (3 of 8 mixes). 2 dB of mean reduction (no further attack gain, more low-band gain change).
- Evidence: The compression probe run record, sections "Section-relative threshold" and "Engine render"; its result files.
- Rule and code references: `compressor::mastering`, `compressor::MASTERING_SECTION_MS`, `compressor::MASTERING_MAXIMUM_DEPTH_DB`; tests in `tests/compressor.rs`.
- Limits and counterexamples: Goal check 4 fails on track-a: the 30 to 200 Hz P05 falls by 0.54 dB against a 0.5 dB bound. That measure counts the compressor gain. The 4 s section window is a set value. On a synthetic 12 dB step between 12 s sections the rule chooses 1.25 dB, because the window straddles each step for 2 s. One loudness (+5.7 LU) measured.
- Verification: The suite, clippy and fmt pass. No listening test.
- Review condition: Measure the limiter gain apart from the compressor gain before the plan-level ducking goal. Measure again at the base loudness.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: The fixed threshold of D124.

### Updates

- 2026-10-16: Keep decision for the listening check Compression. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16. At the base target of D179, the compressor applies its largest mean reduction, 1.50 dB, on the eight album mixes and on regression mix 1; regression mix 2 arrives limited and has no compressor (D130). The album masters lower the crest by 4.05 dB at the median (P25 to P75 3.58 to 4.30 dB) and the 50 ms crest by 0.38 dB (0.26 to 0.50 dB). `master` lowers them by 3.50 and 0.31 dB, with the 50 ms crest of 5 of 8 tracks inside the pair range, and its P90 attack loss is 2.84 dB, under the 3.22 to 4.59 dB of the pairs. The rule removes 0.5 dB or less of window contrast. The compressor and its rule stay. The acoustic decisions run record, "Kept rules".
- 2026-10-18: The update of 2026-10-16 records that regression mix 2 arrives limited. It does not: 0.7% of its active 10 ms blocks are at the plateau, under the 4% of D130. The window-contrast limit of this rule refuses it, because each depth from 1.5 to 0.5 dB removes 0.70 to 0.71 dB of window contrast. It is the first source that the limit refuses. Its masters lose 0.69 to 1.14 dB more energy attack at P90 than the album masters of their version. With the compressor forced to 1.5 dB, the energy attack loss of each version is inside the album range, and the window contrast falls 0.38 to 1.02 dB under the mix. The compression contrast run record. The rule stays until the user decides (TODO.md, "Next").
- 2026-10-18: Superseded by D202 (`set.compression.v3`). The threshold over the section level stays; the contrast limit and the depth search go. The forced render of the update above also took the 3 ms limiter attack of `set.peak-attack.v1`, and the render as made took the instant attack; the difference of energy attack loss comes from both changes.

## D126: Peak control order and clip share: first measurement

- Recorded: 2026-09-29.
- Origin: Step 2 of the mastering plan (D122).
- Status: Accepted. The engine is unchanged.
- Decision: Keep the limiter, then the 2.5 dB soft clipper, at the Denser setting, and keep `set.peak-tone-restoration.v2`.
- Reason: At +5.7 LU with `set.compression.v2` before peak control, no arm meets the criteria set before the run on 6 or more of the eight confirmation mixes. The clip budgets of 1.5 to 3.5 dB pass on 0 to 2 mixes. The clipper before the limiter passes on 1 mix. The engine order with a 1 dB clipper knee and 2.5 to 7 dB of clip depth passes on none. Each arm that lowers the 2 to 10 kHz ducking by 25% or more raises the clipping residual P95 by more than 0.5 dB on 7 or 8 mixes. The largest restoration window change of the present chain is 0.6 dB or more on four mixes.
- Alternatives and tradeoffs: The clipper before the limiter lowers the ducking by 58 to 82% and the energy attack loss P90 to 0.61 to 1.82 dB, against 1.48 to 4.91 dB. It clips the loudest peaks by 7.5 to 9.8 dB, and its presence residual P99 is −0.9 to −9.3 dB. The 1 dB knee with a 5 dB depth lowers the ducking by 49 to 82%.
- Evidence: The peak order run record; its result files.
- Rule and code references: `MASTER_PROBE_ORDER` in the limiter character probe; the peak order summary script.
- Limits and counterexamples: One loudness (+5.7 LU) and one limiter release (50 ms). The residual measure compares power per region and frame and does not include masking. The sample-peak attack loss counts each dB of clipped peak as attack loss; the energy attack loss does not. The measures of the ducking and of the energy attack loss exclude the compressor gain; the plan-level ducking goal uses the ducking measure of this run.
- Verification: Probe only. The engine and the probe chain gave the same output on every mix. No listening test.
- Review condition: A measure of the audibility of the residual and of the ducking against masking; the loudness step (stage 6).
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

### Updates

- 2026-09-29: The masking-model runs keep the clip budgets and the order and change the limiter release to 10 ms (D127).
- 2026-10-16: Keep decision for the listening check Clipper position. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16. At the base target of D179, the total masked NMR of peak control on the `essential` prefix is −8.57 to −14.33 dB on the ten sources of the acoustic decisions run, and the largest limiter reduction of `essential` is 0.11 to 1.76 dB. At a total NMR of 0 dB the disturbance equals the masking threshold (D131), so the disturbance of the limiter and the clipper is 8.5 dB or more under it. In D126 and D127 the clipper before the limiter clipped the loudest peaks by 7.5 to 9.8 dB and raised the NMR by up to 5.6 dB on track-e. The order of the limiter, then the clipper, stays. The acoustic decisions run record, "Kept rules".

## D127: Limiter release of 10 ms from the masked disturbance of peak control

- Recorded: 2026-09-29.
- Origin: Step 2 of the mastering plan; review condition of D126.
- Status: Provisional. Implemented in `src/set.rs` as `set.limiter-release.v1` (`set::LIMITER_RELEASE_MS`).
- Decision: Every set version uses a 10 ms limiter release, with the 12 ms hold and 1 ms windows unchanged. The clip budgets (1.5 dB base, 2.5 dB Denser), the limiter-then-clipper order and `set.peak-tone-restoration.v2` stay.
- Reason: A masking model scores the change that peak control makes to the compressed mix: Schroeder spreading, the Johnston (1988) tonality offset, the PEAQ forward masking and the threshold in quiet. Against the 35 ms of the stem-derived release, 10 ms lowers the total masked NMR by 0.5 dB or more on 7 of 8 confirmation mixes at the base setting (1.5 dB, +4.2 LU) and on 6 of 8 at the Denser setting (2.5 dB, +5.7 LU). It raises it by 0.5 dB or more on no mix at either loudness. The masked NMR under 510 Hz falls on every mix. The median share of frames with the 2 to 10 kHz band 1 dB down falls from 13.1% to 8.0% and from 23.3% to 13.9%. The criteria were set before each run; of the passing releases the longest is kept.
- Alternatives and tradeoffs: 5 ms also passes, with a median total NMR 0.09 to 0.37 dB lower. The clipper before the limiter and the fixed 1 dB clipper knee lower the NMR by 1.5 to 3.3 dB on track-a and track-h and raise it by up to 5.6 dB on track-e; no fixed clip share or order passes. Releases of 100 and 200 ms raise the NMR by 0.5 dB or more on 2 to 7 mixes. The sample-peak P90 attack loss rises by about 0.3 to 0.4 dB at matched loudness; the energy attack loss changes by 0.03 dB or less.
- Evidence: The peak order run record, sections "Masked disturbance run" to "Engine change"; its result files; excerpt sets v0 and v1.
- Rule and code references: `set::LIMITER_RELEASE_MS`; `masked_disturbance`, `peak_disturbance` and `MASTER_PROBE_REFERENCES` in the limiter character probe; the peak bundle summary script; test assertion in `tests/set.rs`.
- Limits and counterexamples: The masking model is not calibrated against listening tests on this material; it omits the PEAQ outer-ear filter and the modulation measures. Two loudness offsets (+4.2 and +5.7 LU) at a fixed gain, not the per-version targets. The best arm differs by mix (clipper first on track-a and track-h, a smaller clip budget on track-e), so a per-recording choice can lower the NMR further. The 12 ms hold applies only to recurring overs. The reference master ducks 25.9% of frames and the masters of an online mastering service 11.0 to 30.9%, with their own compression included.
- Verification: The suite, clippy and fmt pass. The engine and the probe chain gave the same output on every probe render. Excerpt sets rendered before and after. No listening test.
- Review condition: A per-recording choice of the clip share by the masked NMR; the loudness step (stage 6); a listening check of the 10 ms release on bass-heavy material in the phase 5 render.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: `dynamics.stem-release.v1` and `set.stem-recovery.v1` release selection in the set path.

### Updates

- 2026-09-29: Generality check on 18 other tracks: four unused confirmation mixes, seven premixes, and seven synthetic mixes that hold sustained peaks (sub bass at 30, 40 and 55 Hz, 808, guitar wall, swell, sparse acoustic), at +4.2, +5.7 and +7.0 LU. Against 35 ms, 10 ms raises the total or the low-band masked NMR by 0.5 dB or more on no record at either budget (49 and 52 records compared), and lowers the total NMR by 0.5 dB or more on 32 and 14. Seven records at +7.0 LU where an arm misses the target, with limiter means up to 11.8 dB, are left out. The 10 ms release stays. The peak order run record, "Generality check".

## D128: Lead bells on the mid channel, a measured side cut, and no foundation

- Recorded: 2026-09-30.
- Origin: Step 3 of the mastering plan (D122).
- Status: Provisional. Implemented in `src/broad_tone.rs`, `src/width.rs` (`set.side-cut.v1`), `src/set.rs` and `src/selection.rs` (`GateChannel`).
- Decision: The lead presence and low-mid bells of `set.broad-tone.v3` act on the mid channel and are gated on the mid channel; the equal-loudness shelves stay on the stereo mix. The +3/+1.5 dB side-lift ladder is removed. `set.side-cut.v1` cuts the side at 4 kHz when the 2 to 8 kHz side/mid of the source exceeds its 200 Hz to 8 kHz side/mid by 0.5 dB or more, to match them, by up to 3 dB, and never lifts. Stem focus side bells under 500 Hz are bounded to ±1.5 dB after the refinement. No set version applies the shared foundation of `set.all-role-foundation.v1`. Master · Bold scales the stereo shelves and the lead bells.
- Reason: A stereo filter changes the vocal and the accompaniment alike in each band, so the stereo presence bell brightens the mix and leaves the vocal-to-mask ratio (VMR) nearly unchanged. On the eight confirmation mixes the side lift lowers the vocal presence VMR on 6 of 8 mixes by up to 0.77 dB and raises the accompaniment side from 2 to 8 kHz by 2.85 to 4.53 dB. With the new tone the presence VMR is at least that of the D127 Master on 7 of 8 mixes (up to +1.61 dB) and 0.18 dB under it on track-e, and the accompaniment side changes by −1.76 to +0.33 dB. The foundation bells change the mix by 0.06 to 0.31 dB with their envelopes, under the 0.5 dB that a stage must reach. The engineer research notes lower or keep the side high frequencies.
- Alternatives and tradeoffs: A side bell that also lifts a narrow upper side (to the body side/mid) lowers the VMR on track-a and track-g by 0.14 and 0.22 dB. Whole-tone Bold scaling pushes the focus bells of track-g past the 9 dB domain bound. The reference master of track-e widens every region by 2.6 to 4.5 dB; the new Master widens track-e by 1.44 dB, the D127 Master by 2.43 dB.
- Evidence: The tone run record; its result files.
- Rule and code references: `broad_tone::LEAD_BELL_Q`, `width::side_cut`, `set::FOUNDATION_IN_VERSIONS`, `set::LOW_SIDE_BELL_HZ`, `selection::GateChannel`; the tone probe, the tone probe summary script, the set comparison script; tests in `src/broad_tone.rs`, `src/width.rs`, `src/set.rs` and `tests/set.rs`.
- Limits and counterexamples: Goal check 3 fails on two focus bells (track-c mid 1285 Hz +0.44 dB, track-e side 335 Hz +0.45 dB). On track-a the new Master raises the limiter maximum from 7.12 to 7.25 dB, and the Denser version falls back to Denser Essential at the 8 dB bound. The VMR is a masking-model score from stem estimates; no listening test. Bold identifies the lead bells by their Q.
- Verification: The suite, clippy and fmt pass. Complete sets of the D127 and the new engine rendered on the eight mixes. No listening test.
- Review condition: Remove focus bells under 0.5 dB with a per-filter measurement in the engine; the Denser headroom cliff; the confirmation listening render.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: The side-lift ladder of D111; the foundation in the set versions (D104 family).

## D129: Render speed of a complete set

- Recorded: 2026-09-30.
- Origin: The user asked for a lower render time and memory for a complete master set, within a three-hour limit.
- Status: Accepted. Commits 73e3a66 to 2548aaa.
- Decision: Keep the render changes of the render speed run: slice-based and f32-lane oversampler taps, a block-bounded true-peak scan, a resampler with one row of taps per phase, an initial drive that counts the compression once (`set::BASE_DRIVE_OFFSET_DB` 0.5 dB), Illinois interpolation in a bracketed loudness search, a first search moved by the prefix loudness difference against the reference, tonal gates predicted from the source spectrum with a 0.3 dB margin (`selection::PREDICTION_SKIP`), native threads for the true-peak scan, the upsampling and the decimation, and no probe cache in the set renders.
- Reason: A complete track-h set took 219 s on one core and takes 55 s. The eight confirmation sets took 3515 s one core each and take 477 s at four sets at a time. The versions are the same on every set, every version is within 0.03 LU of its target, and every version peaks at −0.50 dBTP. The resampler and the oversampler slicing leave the set outputs identical bit for bit; the f32 lanes change them at the order of 1e-6.
- Alternatives and tradeoffs: A relaxed tolerance for the first render of a restoration added a probe and was reverted. The restoring filters of Essential applied to the other versions would double their remaining tone error (0.24 dB apart on track-f). Parallel versions inside one native set need a change to the ordered delivery and the duplicate screen.
- Evidence: The render speed run record.
- Rule and code references: `dsp::Oversampler`, `dsp::chunked`, `resample::convert`, `search::solve`, `selection::predicted_tonal_change_db`, `treatment_render::Renderer::reference_prefix_lufs`.
- Limits and counterexamples: Timings on an EC2 m8g.16xlarge (Graviton4). The gate prediction is validated on 19 candidates of eight mixes (+0.32 to −0.12 dB). The peak memory, 305 to 467 MB, is unchanged; the stem analysis sets it. The track-a Denser version changes with 0.02 dB of limiter maximum at the 8 dB bound.
- Verification: The suite, clippy and fmt pass. Eight complete sets rendered after each change and compared with the previous render.
- Review condition: Streamed stem analysis for memory; a smooth Denser bound instead of the headroom cliff.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D130: A source that arrives limited keeps its loudness and gets no compressor

- Recorded: 2026-09-30.
- Origin: Step 4 of the mastering plan (D122).
- Status: Accepted. Implemented as `set.input-condition.v1` (`analysis::Temporal.plateau_share`, `plateau_level_spread_db`, `set::LIMITED_PLATEAU_SHARE`, `set::LIMITED_LEVEL_SPREAD_DB`).
- Decision: A source with 4% or more of its active 10 ms blocks within 1 dB of its 99.9th percentile block peak, over a level spread of 1 dB or more, arrives limited. It gets no compressor, its integrated loudness is the base target within −20 to −8 LUFS, and no Denser version is delivered.
- Reason: The 26 unlimited recordings hold 0.35 to 2.39% of their blocks at the plateau and the 23 limited masters 7.06% or more. On 13 limited sources the change removes the compressor, keeps the loudness and removes the Denser version; the confirmation sets are identical bit for bit.
- Alternatives and tradeoffs: The pile-up ratio of the top 1 dB against the next does not separate the groups. The share within 0.5 dB of the largest sample fails on masters with a few samples above their plateau.
- Evidence: The input condition run record.
- Limits and counterexamples: The user set the scope: a master arriving again is not a target case, and the work stops at this rule. No mix that arrives lightly limited was measured.
- Verification: The suite, clippy and fmt pass.
- Review condition: A source class that the rule misreads.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D131: Loudness from the audibility point of peak control

- Recorded: 2026-09-30.
- Origin: Loudness step of the mastering plan (D122) and item 1 of the mastering research notes.
- Status: Accepted. Implemented as `set.audible-loudness.v2` (a cap; v1 set the target at the point) (`audibility::point`, `set::AUDIBILITY_STEP_LU`, `treatment_render::LOUDNESS_TOLERANCE_LU`).
- Decision: Before the render, the set runs its peak control on the compressed mix at probe drives. It finds the output loudness at which the total masked NMR of peak control reaches 0 dB. The base target is 0.5 LU under that loudness, bounded −20 to −8 LUFS. The Denser target is that loudness, capped at −6.5 LUFS and within the base render's limiter headroom. The crest rule `set.source-density.v3` applies when the search finds no point. A source that arrives limited keeps D130.
- Reason: A total NMR of 0 dB is the point where the disturbance of peak control, averaged over the recording, equals the masking threshold. Engineers raise the level until limiting becomes audible and then decrease it in 0.5 dB steps. The crest rule put the base at −0.7 to −10.6 dB NMR on the eight confirmation mixes. The new rule puts it at −1.5 to −3.2 dB.
- Alternatives and tradeoffs: The rate knee of the plan selects the first eligible sweep step on all eight mixes, because the added NMR per LU increases at each step. A cap at the audibility point (base = the smaller of the crest rule and the new target) keeps goal 2 on 8 of 8 mixes but moves the base on two mixes only.
- Evidence: The loudness run record; the cloud sweep results, the audibility point table, set renders 11 and 12.
- Limits and counterexamples: Goal 2 of the step holds on 2 of 8 mixes. The new base is 0.5 to 1.7 LU louder on six mixes, with 1.6 to 7.4 dB more total NMR and up to 1.8 dB more P90 attack loss. The Denser step is 0.5 LU on every mix. The measurement adds 4 or 5 peak-control probes to each set. The masking model has no listening test.
- Verification: The suite passes. The eight confirmation sets deliver the same number of versions as before; on track-a and track-g, Denser Essential replaces Denser Master.
- Review condition: A listening result, or a decision on goal 2.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Replaces `set.source-density.v3` as the base target rule except as the fallback.

### Updates

- 2026-09-30: The user judged the previous masters loud enough and selected the audibility point as a cap (`set.audible-loudness.v2`). The crest rule sets the targets; the base target is at most 0.5 LU under the audibility point and the Denser target at most at it. The base is lower on track-f and track-c by 0.23 and 0.24 LU, and the Denser version is lower on four mixes by 0.14 to 0.87 LU. Goal 2 holds on 8 of 8 mixes.
- 2026-10-16: `set.audible-loudness.v3` (D179) replaces the step of 0.5 LU with a margin of 2.75 LU under the point, the level of the album masters of D177. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- 2026-10-16: Keep decision for the engine item Audibility point per version. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16. An offline probe (`examples/attack_loss_probe`) with `ATTACK_LOSS_AUDIBILITY` searches the audibility point of each delivered version on its own compressed prefix. On the ten sources, each of the 31 versions is 2.27 to 4.92 LU under its own point, and the total masked NMR of its peak control at its loudness is −6.91 to −19.63 dB. The smallest margins are regression mix 1 `master` (2.27 LU) and `open` (2.34 LU) and regression mix 2 `master` (2.62 LU). With the margin of 2.75 LU under the point of `essential` (D179), no version needs a guard of its own. The acoustic decisions run record, "Audibility point per version".

## D132: Stereo widening of the Master and Bold families from the source image

- Recorded: 2026-09-30.
- Origin: User requirement: widening in the style of a human engineer and of a commercial stereo imager, by family, from what the track measures. The reference master is an example, not a target.
- Status: Accepted. Implemented as `set.widening.v1` (`width::side_allowances_db`, `width::widening`, `width::SideGain`, `treatment_render::Spec::side_gain`, `set::bold_candidates`).
- Decision: Each band of `stereo::BAND_EDGES_HZ` from 120 Hz up gets an allowance: the largest side gain that keeps the whole-file correlation drop at 0.30 or less, the P10 window correlation at 0.05 or more, and the extra mono loss at 1.5 dB or less. The Master takes one gain from 120 Hz up, set by the smaller allowance of the two midrange bands and bounded per band. Master · Bold takes each band's allowance, with the 120 to 500 Hz band no wider than the band above. Gains are at most +6 dB. Essential keeps the source image. A zero-phase FIR filter applies the gains to the side channel after tone. The tone's own side/mid change counts toward the gain. The Master keeps the side cut only when it does not widen.
- Reason: The mid and side of the mixes are nearly uncorrelated, so one side gain per band sets the correlation, the mono loss and the side/mid. The selection gates already bound these quantities. The commercial stereo imager widens up to +6 dB of side gain per band. M/S guidance uses linear-phase filters; the engine renders offline, so the filter is zero-phase.
- Alternatives and tradeoffs: Minimum-phase side shelves move the image near their corners. The imager's mono-widening mode is off: the imager's vendor keeps it off on masters that are not nearly mono, and the eight mixes are not.
- Evidence: The width run record; set renders 16.
- Limits and counterexamples: No listening test. Bold equals the Master on track-e and track-d, where the allowances are nearly even. The vocal level at matched loudness falls as the side rises; the spatial release from masking of a centred vocal (2 to 4 dB, headphone figures) is not measured.
- Verification: The suite passes. All eight Masters use the full widening. On track-e the Master is within 0.6 dB of the side/mid of the reference master in each band from 120 Hz to 12 kHz.
- Review condition: A listening result, the comparison with the commercial stereo imager, or a nearly mono source.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Changes D128: the Master widens where the source allows it; the side cut stays for a Master that does not widen.

### Updates

- 2026-09-30: The allowances come from the image after the compressor and peak control at the base target, which widens the image by up to 1.2 dB in a band by itself. The tone counts at its widest (side lift and mid cut in full, side cut and mid lift not at all). The 0.05 margin applies to the window bound too. On all eight mixes, the lowest P10 correlation of each Master is +0.02 or more where the mix's own is positive. The commercial stereo imager at Width 25% goes under 0 on two mixes and at Width 50% on five.
- 2026-09-30: A band whose P10 window correlation is under 0.05 narrows until it reaches 0.05, by up to 6 dB, in the Master and Bold. The narrowing of track-c is 4.1 to 5.8 dB, and its lowest P10 rises from −0.47 to +0.18; a DAW mastering assistant narrows it by about 4 dB to 0.00. All eight Masters keep a positive lowest P10.
- 2026-10-11: `set.widening.v3`. The user asked for bolder treatment until the reference masters of the album give a measured distribution. Two changes: the correlation-drop gate rises from 0.35 to 0.40, so the plan allows a drop of 0.35 (the reference master of track-e drops the two midrange bands by 0.31 and 0.32, at the old planning bound); and the tone's own width change, which the side gain discounts (`SideGain::beside`), is predicted on the source spectrum (`width::tone_width_db`) instead of the mean filter response over the band: side lifts and mid cuts weighted by the side and mid power at each frequency, and stereo filters, which narrow a band when they lift a mono part of it. On track-e the delivered Full version was 1.3 dB narrower than the reference master from 120 to 500 Hz and 1.6 dB narrower than its own band above, with the same planned gain in both bands. The compressor and peak control changed no band's side/mid. Clear of the same set measures −10.4 dB there, at the −10.3 dB of the reference master: the gap of the Full version is its hold, which lifts the mono bass of the band by 3.5 dB so that the band's mid rises more than its side. The mean filter response under-predicted the tone's width: Clear of engine 1.7.0 delivered +3.5 dB in that band from a target of 2.6 dB. Two forms tried first and withdrawn: the low mids at their own allowance bounded by the allowance of the band above (0.4 dB on track-e: the 500 Hz to 2 kHz allowance already bounded them), and the low mids at the midrange gain plus the widening that peak control adds to the band above (0.05 dB: peak control adds none). Result on the eight mixes: the gate binds on track-e and track-d only, whose midrange widens by 0.4 to 0.5 dB; track-e Full is at the reference master from 500 Hz to 6 kHz and wider above. The P10 window bound and the mono-loss bound stay. The sections "track-e against the reference master, engine 1.7.0" and "Widening v3" of the width run record give the measurements. Review: the reference masters of the eight album mixes.
- 2026-10-12: The narrowing of 2026-09-30 is withdrawn. On track-c it took 4.5 to 6.3 dB of side from every band of the whole song (the Master applied the tighter midrange allowance, −4.9 dB, to every band from 120 Hz), and the user judged the mix fine and a master that narrows it not fine. The phase study finds one instrument group polarity-inverted from 0:45 to the end, at zero lag, with the other stems in phase: a mix-stage choice. A band whose P10 window correlation is under the bound now gets no widening and no cut. A per-band narrowing was rendered on the way (track-c Full: 120–500 Hz −1.2 dB, midrange −5.3 and −6.1 dB, 6–12 kHz −1.9 dB, 12–20 kHz −0.1 dB) and withdrawn with the rule. The description of a version reports a side/mid change of 0.3 dB or more against the mix as "Broader" or "More centred stereo balance" (`set::describe_measured`), so a narrower master is named as such. Open: a stem-based source condition that reports an anti-phase instrument group to the user (TODO.md). A DAW mastering assistant narrows this mix by about 4 dB; that stands as a comparison, not a target.
- 2026-10-16: `set.widening.v4` (D183). The band from 120 to 500 Hz of the Master widens by at least +4.7 dB, the median of the album pairs of D177, within its own allowance; Bold widens it up to the larger of the band above and +4.7 dB. The bounds of the allowance and the narrow band under 120 Hz stay. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- 2026-10-16: `set.widening.v5` (D184). In each band with a side in the mix, the widening of the Master stays within the P75 widening of the album pairs and that of Bold within the P90. The bolder widening of 2026-10-11 ends: the album masters give the distribution. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D133: De-esser of the Master family keyed by the vocal stem

- Recorded: 2026-09-30.
- Origin: Step 5 of the mastering plan (D122); the engineer research lists de-essing as part of a bright master.
- Status: Accepted. Implemented as `set.deesser.v1` (`deesser::key`, `deesser::Plan`, `stem_evidence::Evidence.sibilant_frames`, `treatment_render::Spec::deesser`, `fir`).
- Decision: The vocal stem keys sibilant frames (vocal 5 to 10 kHz power above its 1 to 4 kHz power, within 40 dB of the loudest vocal frame and 30 dB of the loudest sibilant power). In a keyed frame the mix's mid channel may carry no more 5 to 10 kHz power than 1 to 4 kHz power; a zero-phase band of the mid is cut by the excess, by up to 6 dB. The stage follows tone and widening, before the compressor, in the Master family.
- Reason: The plan's trigger does not occur: the Master already lowers the sibilant P99 on all eight mixes. The human reference master of track-e lowers the sibilant P99 of track-e by 6.46 dB, to a 5 to 10 kHz power under the 1 to 4 kHz power, with the other vocal frames within 0.54 dB. The rule uses that balance within each recording as its bound; it sets no target from the reference master.
- Alternatives and tradeoffs: A cut on the whole mix would also lower cymbals in the sides. An absolute band-level detector (`treatment::DynamicProbe`) cuts loud passages whether they are sibilant or not.
- Evidence: The de-esser run record; the sibilance probe outputs v1 and v2, set renders 21.
- Limits and counterexamples: No listening test. The 6 dB bound is provisional. The key needs stems; a set without stems gets no de-esser. The sibilant P99 stays above 0 dB on six mixes where the excess exceeds 6 dB or falls outside keyed frames.
- Verification: The suite passes. The sibilant P99 falls by 4.6 to 6.5 dB on all eight mixes; the other vocal frames and the lowest window correlation are unchanged.
- Review condition: A listening result, or a mix without a vocal.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

### Updates

- 2026-10-14: Evidence correction. The −6.46 dB change of the sibilant P99 of track-e in the reference master was measured at a frame lag 44 frames from the master's offset against the mix. At the offset of `analysis::align_offset` (286 samples) the master lowers the P99 by 0.97 dB and the other vocal frames by 0.77 dB; the eight album masters of the same engineer move the sibilant P99 with the other vocal frames within 0.9 dB (the reference album run). No master of this engineer shows keyed de-essing. The rule stands on the engine's own bound and on D170; whether the Master family keeps the stage is open work (TODO.md, "Album pairs").
- 2026-10-16: `set.deesser.v3` (D181). The versions of a set keep the key and the measure and cut nothing; the Sibilance control of a request sets the cut. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D134: Presence balance of the Master family from equal loudness per band

- Recorded: 2026-09-30.
- Origin: A comparison script compares the Master with a DAW mastering assistant and with the reference master. Both lift presence against body by 0.9 to 2.5 dB more than the Master on seven of eight mixes.
- Status: Accepted. Implemented as `set.presence-balance.v1` in `set.broad-tone.v4` (`broad_tone::presence_balance`, `Move::PresenceBalance`).
- Decision: The Master family lifts presence (2 to 5 kHz) against body (500 Hz to 2 kHz) toward the balance of equal loudness per band at 75 phon (ISO 226), −3.30 dB, by at most 2 dB and never down. A stereo bell at 3162 Hz, Q 0.7, adds what the lead bells, shelves and widening do not, by up to 3 dB of filter gain. The lead bells stay on the mid channel; the other moves act on the stereo mix.
- Reason: The lead-share rule sees the vocal only; the dull mix of track-e got no lift. The equal-loudness reference comes from the ear, not from a corpus; the moves of the DAW mastering assistant imply a similar balance (about −2.7 dB).
- Alternatives and tradeoffs: Equal power per octave (pink) gives 0 dB and would lift track-a, the brightest mix. Half the shortfall would add a free constant.
- Evidence: The presence run record; set renders 23.
- Limits and counterexamples: No listening test. The prediction uses the long-term source spectrum; the side narrowing of track-c lowers presence more than predicted. The lead bell still lifts presence on track-a, which is above the reference.
- Verification: The suite passes. The Master is within 0.4 dB of the DAW mastering assistant on six mixes; sibilance and the lowest window correlation hold.
- Review condition: A listening result.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Extends `set.broad-tone.v3` (D113, D128).

### Updates

- 2026-10-16: `set.presence-balance.v2` (D180) bounds the whole static tone, the lead bell included, at the P75 move of the album pairs (+0.94 dB) and at the equal-loudness balance. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D135: Versions differ at matched loudness; no Denser version

- Recorded: 2026-09-30.
- Origin: Step 7 of the mastering plan (D122).
- Status: Accepted. Implemented in `selection::Signature::is_duplicate_of` (`selection::DISTINCT_DB`) and `set::DENSER_VERSIONS`.
- Decision: A version is a duplicate unless it differs from a delivered version by 0.5 dB or more at matched loudness in a band, in side/mid or in mean limiter reduction. The delivered level alone does not count. No Denser version is planned. A set delivers Essential, Master and Master · Bold.
- Reason: Under the loudness cap (D131) each Denser version is 0.49 to 0.72 LU above its base and differs by 0.05 dB or less in each other measure. Loudness-normalised playback removes the level difference. The plan sets 0.5 dB as the distinct difference; the previous screen used the description thresholds (0.4 dB band, 0.3 dB side/mid, 0.15 dB limiter, 0.3 LU level).
- Alternatives and tradeoffs: Keep planning Denser and let the screen drop it: two renders per set that are never delivered. A larger Denser step would pass the audibility point that D131 caps.
- Evidence: The versions run record; set renders 23, 24 and 25.
- Limits and counterexamples: A listener who does not normalise loudness loses a 0.5 LU louder option. The app and the cloud workers still accept Denser roles in a plan.
- Verification: The suite passes. All eight sets deliver Essential, Master and Master · Bold; set render time falls from 75–120 s to 45–93 s.
- Review condition: A loudness rule that lets a Denser version differ in density by 0.5 dB or more.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Changes the version set of D111 and the dense bound of D115.

## D136: Version names Light, Balanced and Bold

- Recorded: 2026-09-30.
- Origin: User requirement: every version is a complete master, so the names must not call one of them "Master".
- Status: Accepted. Implemented in `set::ESSENTIAL_LABEL`, `set::MASTER_LABEL` and `set::BOLD_LABEL`.
- Decision: The versions are named Light (IDs `essential`), Balanced (`master`) and Bold (`bold`), from the lightest treatment to the strongest. The IDs stay the same, so saved sets and the app keep working. Descriptions compare with Light.
- Reason: Light keeps the mix's own tone and image; Balanced adds the measured presence, width, low-mid and de-essing moves; Bold goes further with the same moves.
- Alternatives and tradeoffs: Natural, Balanced and Bold. The user chose Light.
- Evidence: None; a naming decision.
- Limits and counterexamples: The documents before this entry use Essential, Master and Master · Bold.
- Verification: The suite passes.
- Review condition: None.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Changes the labels of D111. Superseded by D157.

## D137: Removal of the legacy render pipeline

- Recorded: 2026-09-30.
- Origin: Step 8 of the mastering plan (D122).
- Status: Accepted. Implemented in `src/main.rs`, `src/pipeline.rs`, `src/set.rs`, `src/compressor.rs`, `src/imaging.rs`, `src/finishing.rs` and `src/bench.rs`.
- Decision: Remove the code that no delivered version uses. The CLI keeps `set`, `set-plan`, `render-family` and `inspect`. The `render`, `plan`, `album` and `bench` commands are removed. `album.rs` and `intent.rs` are removed. `pipeline.rs` keeps `SourceAnalysis`, `analyze_source`, `Constraints`, `diagnostic_constraints` and `violations`. `compressor.rs` loses its planning, trial and benefit code. `imaging.rs` keeps `Check`. `finishing.rs` loses `plan`, `proposals` and `stem_proposals`. `bench.rs` keeps the tone signal and tone measurements. The examples `compression_probe`, `loudness_probe` and `listening_render` are removed. `audibility_point` sets its limiter directly.
- Reason: The set replaces the single-render pipeline (D109). The set used the genre intent only for a limiter setting and the diagnostic constraints. `set::prepare` now sets the limiter directly: the default limiter with a 10 ms release. `pipeline::diagnostic_constraints` gives the constraints of punch 0.7 and intensity 0.8.
- Alternatives and tradeoffs: Keep the legacy commands for the historical probes. The scripts that call `master render` or `listening_render` (for example) need a revision to run after this change.
- Evidence: The track-e excerpt with its four stems, before and after the removal. The `set-plan` JSON is byte-identical (SHA-256 `fe8a8b4b6da6cc2a81b60b6858bfe341470df798c32a0e395b398892b627af22`). The `inspect` JSON is byte-identical (`8995ea3b6d9659f1d5d41b1e99ae95f91abf05fd1721e65b3ba25eda353dd492`). The three delivered WAVs are bit-identical: `essential` `958360afa307cc6b344d5fecddd0238651b6b0a6a527ac839c3bf442d30c9eeb`, `master` `11567b9e85f964d0bd52e624bc4a6012bbdc4c527e56d1aa0e7fead4e51302cc`, `bold` `e04fc8709d838e0d7606a18b60ab583615124de2c82244b176c886cb09024521`. The version reports, `analysis-and-trials.json` and `set.json` are identical apart from timing and the renderer hash.
- Limits and counterexamples: One excerpt is the evidence. `dsp::limit`, `finishing::color`, `finishing::clip` and `stem_evidence::analyze_sequential` stay as references for tests and probes.
- Verification: `cargo fmt --check`, `cargo clippy --release --all-targets` with no warnings and `cargo test --release` pass. `mastering-wasm` and the Lambda crate compile.
- Review condition: None.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Removes the implementation of the intent, imaging, compression-planning, finishing-selection and album rules. `docs/RULES.md` marks them "Removed (D137)".

## D138: Cloud stems aligned and admitted as stem evidence

- Recorded: 2026-09-30.
- Origin: The end-to-end cloud test after the D133–D137 deploy: no cloud version carried the de-esser, the vocal bells or the focus bells.
- Status: Accepted. Implemented in the Lambda source module (`opus_delay_frames`, `decode_stem`) and `stem_evidence::STEM_SUM_RESIDUAL_DB`.
- Decision: The cloud decoder removes the 2.5 ms look-ahead (120 samples at 48 kHz) from each Opus stem, as the splitter's transport does (`PRE_SKIP`). The stem sum passes the evidence check at a residual of −15 dB or lower, with the cosine bound of 0.98 unchanged.
- Reason: The decoded stems started 120 samples late: the stem sum's cosine with the mix was 0.25, and the evidence was unusable on every cloud job. Aligned, the cosine is 0.991. The 192 kbps Opus coding leaves a residual of −17.6 dB, against about −31 dB for lossless stems; the cosine bound still rejects stems from another mix or out of alignment.
- Alternatives and tradeoffs: Lossless stems in the cloud: about eight times the transfer per job.
- Evidence: The Lambda stem lag probe and stem usability probe on a cloud job: lag 120 samples before, 0 after; usable, with 443 sibilant frames.
- Limits and counterexamples: The decisions that the stems guide are not compared between Opus and lossless stems of one mix.
- Verification: The suite passes; the cloud end-to-end test follows the deploy.
- Review condition: A change of the splitter's codec or pre-skip.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D139: Material bands by raw power or by equal-loudness weighted power

- Recorded: 2026-10-01.
- Origin: Source observation. Stress recording 1, a live recording, received Light only.
- Status: Accepted. Implemented in `selection::material_bands`, `stereo::BandMetrics::weighted_power` and `broad_tone::equal_loudness_weight`.
- Decision: A band is material when it is active and holds 1% of the raw power of the strongest band, or 1% of the total power weighted by the inverse of the 75-phon equal-loudness contour (`REFERENCE_PHON`).
- Reason: On stress recording 1 the 20–120 Hz band holds most of the raw power, and the 2–6 kHz band holds 0.22% of it. The presence gate had no measurement for that band, so each Balanced candidate failed and Bold had no base. The 75-phon weighted share of the 2–6 kHz band is 2.66%. The presence balance (D134) uses the same monitoring level.
- Alternatives and tradeoffs: The K-weighted share, as the restoration uses (D120): 0.61% on stress recording 1. The 75-phon weighted share alone: the 6–12 kHz band of each confirmation mix falls to 0.20–0.48% and leaves the gates and the duplicate screen.
- Evidence: The live run record.
- Limits and counterexamples: One live recording. The 6–12 kHz band of stress recording 1 stays under 1% in both tests.
- Verification: The suite passes. Engine 0.2.0 and this engine render identical WAVs and descriptions for the eight confirmation sources without stems. Stress recording 1 delivers Balanced and Bold.
- Review condition: A recording whose quiet band carries a move that the gate cannot measure.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Changes the material band test of D111.

## D140: A version that matches the mix is not delivered

- Recorded: 2026-10-01.
- Origin: User requirement: "if its the same dont even include it".
- Status: Accepted. Implemented in `set::same_as_mix` for the native, WASM and cloud sets.
- Decision: A version is not delivered when its delivered level is within 0.5 LU of the mix and the duplicate screen (D135) finds no difference of 0.5 dB or more against the mix. Its status is `duplicate`, with `duplicate_of` set to `original`. When Light is not delivered, the other descriptions compare with "your mix". The Light description states the measured level change and peak control.
- Reason: A limited source keeps its loudness (D130). When its peaks are under the ceiling, Light applies no drive and no limiter reduction and matches the mix: −102 dBFS RMS residual on stress recording 1. The duplicate screen alone ignores level, so it would also remove a louder Light.
- Alternatives and tradeoffs: The masked noise-to-mask ratio of the change against the mix (`audibility::peak_control_nmr_db`): it fits a gain per window, so a louder Light at the audibility point also counts as unchanged.
- Evidence: The live run record.
- Limits and counterexamples: A set where every version matches the mix delivers no version.
- Verification: The suite passes. Each confirmation Light is 2.3 to 4.4 LU louder than its mix and is delivered.
- Review condition: None.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Extends D135.

### Updates

- 2026-10-01: The cloud family worker publishes no Light preview for a Light that matches the mix (`distributed::render_family_with_callback`). The app rejects a job whose finalizer removes a previewed version (`bitneedle-app/web/js/mastering.mjs`). A family preview probe prints the preview decision: the stress recording 1 excerpt has none, the track-e confirmation mix has one.

## D141: Stereo width for a mono source from a decorrelated mid

- Recorded: 2026-10-01.
- Origin: User requirement after the assessment of the video master of stress recording 1: mono-safe width for a mono recording.
- Status: Accepted. Implemented in `width::SideGain::decorrelated_db`, `width::decorrelated_db`, `width::decorrelation_sign` and `fir::quadrature_kernel` (`set.widening.v2`).
- Decision: Where the side gain of a family (D132), bounded at 6 dB, reaches less than half of the side power that the band's allowance permits, the side receives the rest as a decorrelated mid. The decorrelated mid is the mid with a 90° phase shift whose sign alternates in each ERB. Balanced and Bold use their own desired gains; a fallback uses half of the decorrelated power.
- Reason: Stress recording 1 has no side signal, so a side gain cannot widen it. The 90° shift has no correlation with the mid at any frequency, so the channels keep equal levels in every band and passage, each channel keeps a flat magnitude, and the mono sum is the mid. The alternating sign keeps the image centred.
- Alternatives and tradeoffs: A velvet-noise decorrelator: correlation with the mid of −0.76 to +0.50 per band and a third-octave ripple of up to 9.9 dB. A 180° phase rotation per ERB: channel differences of up to 0.55 dB. A delay (Haas): comb filtering in each channel.
- Evidence: The live run record.
- Rule and code references: `width::DECORRELATE_SHARE`, `selection` correlation and mono gates.
- Limits and counterexamples: One mono recording. 0.55% of the kernel energy is more than 10 ms before the centre; its audibility on transients is not measured. Balanced and Bold have the same width on a mono source.
- Verification: The suite passes. Stress recording 1: correlation 0.70–0.73 and side/mid −8.0 to −7.6 dB from 120 Hz up, channels within 0.03 dB, after the correction of the kernel's band loss (`width::decorrelation_loss_db`). The confirmation sources render identical WAVs and descriptions with and without stems.
- Review condition: A listening comparison of the width with the video master and with the mono-widening mode of a commercial stereo imager.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Extends D132.

### Updates

- 2026-10-16: Keep decision for the listening check Mono-source width. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16. A decorrelator pre-echo probe measures the decorrelated mid at −7.5 dB from 120 Hz up, the level of stress recording 1. Before a click after 1 s of silence, the left channel is −5.3 dB against its first 5 ms within 1 ms of the onset, −24.9 dB from 5 to 10 ms, −33.4 dB from 10 to 20 ms, −46.8 dB from 20 to 40 ms and −99.4 dB from 40 to 85 ms; before a noise burst it is −17.2, −29.3, −35.1, −49.2 and −117.5 dB. Premasking covers about 20 ms before an onset. A kernel of 2049 taps removes the part from 20 to 40 ms (−81.7 dB) but loses 4.58 dB of the decorrelated mid from 120 to 500 Hz. Since D184, a band of a stereo mix takes no decorrelated mid; no source of the regression set uses it. The 4097 taps and the level of the decorrelated mid stay. The acoustic decisions run record, "Kept rules".
- 2026-10-16: The WIDTH tab of the web MASTER screen shows the decorrelated mid of a version (`spec.side_gain.decorrelated_db`, bitneedle-app `22f3d20`). The band table has a column "From centre" with its level against the mid in each band. The words of the tab are "In the bands marked under From centre, your mix has too little stereo for a lift of its sides to widen it, so this version makes width from the centre of the mix. On a single speaker that width cancels, and the centre plays as it is." The tile summary is "From centre". The native app does not show it.

## D142: Source condition report

- Recorded: 2026-10-01.
- Origin: User requirement: detect a live or phone recording and show the detection as a mastering decision on the MASTER screen.
- Status: Accepted. Implemented in `source_condition::measure` (`set.source-condition.v1`); `Summary::source_condition` and `set.json`.
- Decision: The set reports how the recording was captured: mono (no band carries a side signal), the bandwidth of a codec low-pass (a fall of 30 dB or more within a sixth of an octave from 8 to 20 kHz), the input condition of D130, and the anti-phase bands (phase between the channels beyond 90°, phase resultant 0.5 or more, whole-file correlation −0.5 or less). No stage reads the report.
- Reason: The capture plan needs measured conditions before a stage can depend on them. A device setting or a codec, not a person, sets these properties. The thresholds separate the corpus from the stress set: the largest fall of a confirmation mix is 19.1 dB, of a phone or codec recording 37.8 dB or more; the lowest band correlation of a confirmation mix or of a stress recording without the fault is −0.26, of an anti-phase band −0.52 or less.
- Alternatives and tradeoffs: A label "Live recording": no measure separates a live recording from a studio recording. A label from the file metadata (device model, codec): the engine receives PCM; the app holds the metadata.
- Evidence: The capture plan, section "Source condition".
- Limits and counterexamples: Three anti-phase recordings come from one phone in one session. The bandwidth search stops at 20 kHz.
- Verification: The suite passes. The eight confirmation sets render identical WAVs and descriptions.
- Review condition: A recording whose cutoff or anti-phase band is between the corpus and the stress set.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

### Updates

- 2026-10-01: D143 and D146 read the report. The worker plan (`mastering.worker-plan.4`) carries it, and the cloud manifest (`mastering.distributed-set.2`) reports it as `source_condition`. The app writes the note of the Original card from it: a recording that is mono, carries a codec low-pass, or has an anti-phase band is named a phone, camera or messaging-app recording, with one sentence for each measured condition.
- 2026-10-16: `set.source-condition.v3` (D187): the report gives the DC offset of each channel. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- 2026-10-16: The copy of the web Original card for a captured recording (bitneedle-app `b4216c4`, `originalNote` in `web/js/mastering-source.mjs`), against the UI copy rules of the workspace. The sentence "This sounds like a phone, camera or messaging-app recording, so every version is mastered as a live recording." named a treatment that the engine does not have: a captured recording has no path of its own (D166), and its tone gets no equal-loudness move (D146). The card now reads, one sentence for each measured condition: "The original recording, aligned with every version. No mastering changes." "It sounds like a recording from a phone, camera or messaging app, so the masters do not change its bass and treble to suit their louder level." "It is in mono." "Its treble stops at 16.0 kHz, so no version adds treble above that." (the measured frequency) "Some of its sound cancels out on a single speaker, so every version lines the two channels up again." The last sentence said "a mono speaker"; "a single speaker" is the word of the copy that the user agreed for D176. A mix that is not a captured recording keeps "The original mix, aligned with every version. No mastering changes." The native card (`Bitneedle/MasteringStore.swift`) keeps the earlier copy. The agreement of the user for the copy is open.
- 2026-10-20: `set.source-condition.v4`: the report gives `input_quiet`, true for a source at −24 LUFS or less (`source_condition::QUIET_LUFS`). `SourceCondition::device_capture` reads it for the capture stages (D200, D201). Basis: user decision of 2026-10-20.

## D143: Phase alignment of a source with an anti-phase band

- Recorded: 2026-10-01.
- Origin: Source observation. Three phone recordings of one session (stress recordings 3, 4 and 5) hold a phase of 157° to 173° between the channels above 6 kHz, and their high band cancels in mono.
- Status: Accepted. Implemented in `stereo_phase` (`set.phase-alignment.v1`), `Spec::phase_alignment` and `set::prepare`.
- Decision: When the source condition reports an anti-phase band (D142), the set measures the whole-file cross-spectrum phase per twelfth of an octave, smoothed over a sixth of an octave and unwrapped inside each turned region. Where the phase resultant is 0.5 or more and the phase is beyond 90° (from 60° with a raised-cosine transition), the left channel turns by minus half and the right channel by plus half of the phase, by a 4097-tap FIR. Magnitudes do not change. The turn is the first stage of each version, and the compressor, the audibility point and the widening measure the aligned mix.
- Reason: A consistent offset of this size cannot come from microphone spacing, whose phase changes with the source and wraps with frequency. The offset removes the high band from the mono sum and puts the vocal presence in the side. The confirmation mixes and the other stress recordings hold 17.5° or less at a resultant of 0.9 or more, and none reports an anti-phase band.
- Alternatives and tradeoffs: Narrowing the band with the width stage: the engine lowered 12–16 kHz by 1.2 to 4.8 dB on stress recordings 3 and 4, because the side holds most of the band. A polarity inversion of one channel above a crossover: the phase rises from about 90° at 3 kHz to 170° at 14 kHz, so one inversion leaves part of the region turned. The prototype threshold alone (resultant 0.5) without the D142 condition: it turned parts of stress recordings 6, 7 and 8.
- Evidence: The capture plan, section "Anti-phase high band"; a phase alignment prototype script.
- Limits and counterexamples: One phone and one session. The true peak rises when the channels add: +1.1 to +2.9 dBTP on stress recording 5 before peak control.
- Verification: The suite passes. Stress recording 5 without stems: correlation above 2 kHz from −0.07…−0.97 to +0.22…+0.97; the set delivers Light and Balanced instead of Light only. The confirmation sets render identical WAVs and descriptions with and without stems.
- Review condition: A recording whose anti-phase region is an intended effect.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D144: A move whose gate cannot measure it is not proposed

- Recorded: 2026-10-01.
- Origin: Source observation. Stress recordings 6 and 7 delivered Light only.
- Status: Accepted. Implemented in `broad_tone::Analysis::retain_measurable`; `Analysis::unmeasurable` lists the dropped moves.
- Decision: The set keeps a tonal proposal only when each band of its gate is material in the source (D139). The equal-loudness air move on a source with a codec low-pass has a gate on 12–20 kHz, which holds no material energy.
- Reason: A gate without a measurement fails each candidate of the version. On stress recordings 6 and 7 the air move's gate had no measurement, so Balanced and Bold failed although every other gate passed.
- Alternatives and tradeoffs: Pass a gate that cannot measure: the move would then render without a check.
- Evidence: The capture plan, section "Delivery".
- Limits and counterexamples: None known. No confirmation mix loses a proposal.
- Verification: The suite passes. Stress recording 7 without stems delivers Light, Balanced and Bold. The confirmation sets render identical WAVs and descriptions with and without stems.
- Review condition: None.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: Extends D139.

## D145: Recordings of up to 30 minutes

- Recorded: 2026-10-01.
- Origin: User requirement: master the long live recordings of the stress set. The app refused a track over five minutes.
- Status: Accepted. Implemented in `render_blocks::render_segmented`, `render_blocks::upsample_into`, `render_blocks::render_peak_control`, `dsp::LimiterStatsAccumulator::merge`, `audibility::point`, the stack template and the stemsplits whole-track worker.
- Decision: Peak control of a recording longer than five minutes renders in 60 s segments of the 4x stream on the available cores. Each segment starts 1 s early and ends 16,384 4x frames late, and keeps only its core's samples and statistics. The audibility point and the widening keep their 4x prefix in scratch storage. The planner, family and finalizer Lambdas have 3,008 MB, the largest function memory of the account, and 10,240 MB of temporary storage. The stem worker keeps 48 segment requests in flight. The cloud accepts recordings of up to 30 minutes; mastering on the device keeps five minutes.
- Reason: A whole-file render is limited by the 900 s Lambda timeout and the memory of the function. On the 3.6-minute track-h job the stem worker took 444 s with eight segments in flight; 26 minutes would take about 3,100 s. The audibility point held its 4x prefix in memory: 2.4 GB for 26 minutes.
- Alternatives and tradeoffs: Chunked mastering with per-chunk decisions: each song of a set would reach the same loudness, and the set's dynamics would be lost. Per-chunk renders with whole-program measurements combined from the chunks: the same result at a higher orchestration cost; it remains the route past 30 minutes and past the function memory.
- Evidence: Segmented and whole renders: bit-identical on the test signal; on track-e 2 to 8 of 22.4 million samples differ by one 24-bit step (residual −192 to −198 dB) with identical decisions, so the segmented path starts above five minutes. The full stress recording 1 (25:51) renders in 114 s against 124 s on 8 cores, with a peak resident memory of 2.5 GB against 3.4 GB before.
- Limits and counterexamples: Memory still grows with duration: full-rate buffers remain. The account allows 3,008 MB per function; a 26-minute set needs about 2.5 GB natively.
- Verification: The suite passes. The confirmation sets render identical WAVs: a recording of five minutes or less renders as one stream.
- Review condition: A recording over 30 minutes, or a Lambda report near the memory or time limit.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

### Updates

- 2026-10-01: The full stress recording 1 (25:51) completes in the cloud in 18 min 41 s from submit: stems 4 min 13 s, planner 3 min 11 s, families 6 min 39 s, finalizer 1 min 55 s. Lambda reports 1,923 MB (stems), 2,531 MB (planner), 2,995 MB (families) and 2,997 MB (finalizer) of the 3,008 MB. Before, the Balanced family peaked at 4.96 GB on two cores and the stem worker at about 3.5 GB; both ran out of memory. Changes: the filters of the widening, the phase alignment and the de-esser stream through `fir::filter_each` and hold no full-length f64 copy; the family releases the audio of its reference and of its base version; scratch reads use a 64k-frame buffer; the decoded source has no Vec slack; the stem worker frees its buffers early, resamples by block, encodes two stems at a time and keeps 128 segments in flight. On two cores the Balanced family peaks at 2.17 GB. The eight confirmation sets render identical WAVs and trials.
- 2026-10-01: The delivery check decodes the written WAV block by block (`delivery::verify`), with a loudness meter and a 4x peak estimate over cores that give the same values as the whole-file measurement. The finalizer frees the delivered WAV after its waveform. On two cores the Balanced family of the full stress recording 1 peaks at 1.96 GB and the Light family at 1.53 GB. The eight confirmation sets render identical WAVs, trials and delivery figures.
- 2026-10-02: The full stress recording 1 completes in 11 min 21 s from submit: stems 4 min 05 s, planner 1 min 31 s, families 4 min 09 s (D147), finalizer 1 min 36 s. Peak resident memory: planner 2,006 MB, families 1,507 to 1,960 MB, finalizer 1,730 MB, of 3,008 MB. Lambda's reported memory also counts the /tmp page cache; each function logs its own peak. Changes: SoundKit decodes and writes audio without quadratic copies; SHA-256 uses the Arm SHA-2 instructions; `stft::Stft` transforms without allocation; the stereo analysis, the spectrum, the stem evidence and the masking frames transform on the available cores in order; the de-esser key reads the vocal stem in place; a decoded stem reserves its length. Every change keeps the WAVs, the plans and the reports identical: the eight confirmation sets and the full stress recording 1 render identical bytes.
- 2026-10-05: Peak control renders in segments for each recording longer than one segment (60 s), not only above five minutes (D163). The Natural family of track-f takes 15.2 s against 18.4 s on two cores.
- 2026-10-16: The source analysis (`pipeline::analyze_source`) takes about 0.5 s for each minute of audio with two threads on an M1: 1.33 s for regression mix 2 and 14.99 s for a 29.9-minute file. Each family and the finalizer make it. The finalizer reads only the metrics and the stereo image of the source and now measures only those (`pipeline::measure_mix`): 6.75 of 14.99 s on the 29.9-minute file, with identical manifests. A pass of the whole analysis from the planner is not made: it needs an exact encoding of the temporal blocks, and it saves at most 0.5 s for each minute of audio in each family.
- 2026-10-16: The plan makes one 4x prefix for the audibility point, the base render and the attack render, and frees the base render before the attack render. With two threads, the plan of a 10.9-minute file peaks at 1,749 MB against 2,501 MB, and the plan of a 29.9-minute file at 3,637 MB against 4,249 MB (macOS physical footprint). The stem analysis is now the peak of the 29.9-minute plan, at 3,544 MB, over the 3,008 MB of the planner Lambda. The stems of that file are at 44.1 kHz and the mix at 48 kHz.
- 2026-10-16: In Node and in headless Chromium, the WASM set of track-e (3.9 minutes) with its stems reaches 1,334 MiB of linear memory in the plan. With two threads natively, the plan of track-e peaks at 687 to 833 MB. On wasm32, `scratch::temporary` keeps the 4x prefix of the audibility point in memory, 358 MB for track-e.

## D146: No equal-loudness compensation for a captured recording

- Recorded: 2026-10-01.
- Origin: Source observation on the stress set.
- Status: Accepted. Implemented in `source_condition::SourceCondition::capture` and `set::prepare`.
- Decision: A source that is mono, carries a codec low-pass, or has an anti-phase band is a captured recording. The tone stage gets a delivered gain of 0 LU for a captured recording, so the equal-loudness compensation of D116 adds no move.
- Reason: D116 keeps the balance that a person heard at the level of the mix. A phone or camera sets the level of a captured recording, and no person balanced the recording at that level. At a delivered gain of 13 to 22 LU, the compensation lowered 20–60 Hz by up to 3.5 dB on recordings whose microphones already roll off the low band.
- Alternatives and tradeoffs: An equal-loudness reference for the low band against the body: the reference of +20.4 dB is 9 to 32 dB above each recording of the stress set, so it gives no physical target. A measured low-band lift for a thin capture: open (TODO.md).
- Evidence: Without stems, before and after. Stress recording 7: the bass proposal of −2.0 dB is removed; Balanced and Bold no longer lower the bass by 1.9 and 3.2 dB. Stress recording 6: −2.0 dB removed; stress recording 5: −0.81 dB; stress recording 3: −0.52 dB. The presence proposals stay within 0.2 dB. A DAW mastering assistant at its neutral setting raises 20–60 Hz on each of these recordings by 0.8 to 7.9 dB.
- Limits and counterexamples: The engine does not yet lift the low band of a thin capture. A studio mix that is mono, or that has a codec low-pass, also gets no compensation.
- Verification: The suite passes. Each of the eight confirmation mixes measures as full band, stereo and without an anti-phase band, so its decisions and WAVs are unchanged.
- Review condition: A mastered capture reference that lowers the low band, or a studio mix classed as a capture.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None. Limits D116.

### Updates

- 2026-10-17: A captured recording keeps the delivered gain of 0 LU. D200 lets its presence move reach the darkest album master, and D201 moves its low band into the range of the album masters with low shelves. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D147: Bold renders in its own cloud function

- Recorded: 2026-10-02.
- Origin: Engineering proposal: shorten a 26-minute cloud job.
- Status: Accepted. Implemented in `set::PreparedSet::worker_plan`, `distributed::render_family` and `distributed::finalize_set`.
- Decision: The worker plan has a third family, `bold`, whose Bold version scales the first master candidate (`speculative_base`). It renders beside the master family. The master family renders Bold only when it chooses another candidate (`unless_base_candidate`). The finalizer keeps the speculative Bold only when the master family chose that candidate and rendered no Bold of its own. The speculative Bold writes `bold-speculative.wav`; its dither seed is that of `bold`. The native set renders Bold after its base, as before.
- Reason: The master family rendered the Light reference, Master and Bold in order: 163 s on two Graviton4 cores for the full stress recording 1, 360 s in Lambda. Bold needs only the chosen master spec, which is a plan candidate.
- Alternatives and tradeoffs: A Bold function that waits for the master family's choice: no time saved. Sharing the Light reference measurement between families: each family still waits for it.
- Evidence: Full stress recording 1 on two cores: master family 98 s, Bold family 111 s, against 163 s for both; the WAVs of both are identical to before.
- Limits and counterexamples: A master family that chooses another candidate renders Bold after its base, as before, and the speculative Bold is discarded.
- Verification: The set tests compare every delivered WAV of the cloud path with the native set, for a set whose master chooses the first candidate and for one whose master chooses another. The suite passes. The eight confirmation sets render identical WAVs and JSON.
- Review condition: A Bold that depends on more than the chosen master spec.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

### Updates

- 2026-10-16: `set.bass-version.v3` (D164). The finalizer also keeps the speculative Bold that passed its gates when the master family chose no candidate. The native set then renders Bold from the planned first master candidate without the bass step, the spec that the speculative Bold scales. A master family that chooses the first candidate without the bass step also keeps the speculative Bold.
- 2026-10-16: A native set renders `master`, `bold` and `open` beside each other after `essential`, for a recording of up to five minutes (`set::PARALLEL_VERSIONS_SECONDS`). `bold` renders from the planned first `master` candidate, as the speculative Bold of the cloud. It starts when `master` renders that candidate first, and it stops before its next render when `master` renders another candidate. The set keeps the render of `bold` when `master` chooses the first candidate or none and the candidates of `bold` render alike (`set::same_candidates`); otherwise `bold` renders after `master`, as before. The events, the comparisons, the deliveries, the trials and the work counts keep the order of the plan. The nine sets of the regression set give identical WAVs and JSON. The browser engine renders in order. A host that stops a native set during those renders waits for them.
- 2026-10-16: A family result (`mastering.worker-family.2`) records each version whose ladder chose no candidate in `unchosen`, with its candidate trials and the failed checks of each trial (`distributed::UnchosenVersion`, `distributed::FailedGates`). The finalizer requires the schema and puts both fields in the `no-opportunity` trial of the version. A speculative Bold that stays out because the master family chose another candidate takes the trials of the Bold of the master family. Before, the manifest gave no reason for such a version. On the 2026-10-15 job of regression mix 2 (engine 1.8.0), the record names the tonal gates that each `master` candidate failed. The delivered WAVs do not change. `tests/set.rs`, `a_version_that_chooses_no_candidate_keeps_its_trials_and_failed_gates`.

## D148: SIMD and CPU-specific release builds

- Recorded: 2026-10-02.
- Origin: User requirement: use every available build optimization. Browsers without WebAssembly SIMD are not supported.
- Status: Accepted. Implemented in the Cargo configuration and manifest of the WASM package, and in the Cargo configuration and manifest of the Lambda.
- Decision: The browser engine builds with `-C target-feature=+simd128`, the rustfft `wasm_simd` feature, fat LTO and one codegen unit. The Lambda functions build for `aarch64-unknown-linux-gnu` with `-C target-cpu=neoverse-n1`, fat LTO and one codegen unit. The app tells a browser without WebAssembly SIMD to update, or to use cloud processing. The app verifies cloud downloads without the mastering module on such a browser.
- Reason: The true-peak oversampler is about a third of the plan's CPU time. Its 16-lane loops compile to SIMD only when the target has SIMD. The FFT uses SIMD in the browser only with `wasm_simd`. Lambda arm64 runs on Graviton2 (Neoverse N1) or a newer Graviton.
- Alternatives and tradeoffs: A second, scalar browser build for Safari before 16.4: two builds to maintain and a slow engine on the oldest phones. The Neoverse V2 target of Graviton4: Lambda can run on Graviton2. The root `Cargo.toml` keeps its release profile, so that `cargo test --release` builds fast.
- Evidence: The WASM bench record, "Build flags": 30% faster plan and 37% faster render in Node on an M1, and 40% and 46% on Graviton4, with an identical output file. On an M1, the native Neoverse N1 build with fat LTO planned about 7% faster, with an identical plan and six identical rendered files. On Graviton4, the Lambda build was under 2% faster, with identical output.
- Limits and counterexamples: rustfft `wasm_simd` changes plan values in the last bits (about 1e-15 dB in six side-gain bands of the bench set). The scalar WASM plan and the native plan already differ in the same values. Fat LTO makes release builds of the Lambda crate and the browser engine slower. Safari before 16.4, Chrome before 91 and Firefox before 89 cannot load the browser engine.
- Verification: The bench set and two corpus mixes give identical output files in the scalar and the D148 browser builds. The Lambda image builds with the new flags. Lambda timing is not measured.
- Review condition: A browser target that must support Safari before 16.4. A Lambda architecture other than Graviton.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D149: Real-input FFT for the stereo spectra

- Recorded: 2026-10-02.
- Origin: User requirement: make the FFT faster, measured on a Lambda-class host.
- Status: Accepted. Implemented in `stft::StereoStft`, `stereo_phase::measure`, `fir::filter_each` and `source_condition::bandwidth_hz`.
- Decision: The stereo spectra (masking frames, auditory measure, stem evidence, long-term spectrum, stereo analysis, restoration window bands and phase alignment) use one real-input FFT (`realfft`) per channel. The FIR filter and the bandwidth measure transform out of place with one scratch buffer.
- Reason: The FFT is about 18% of the plan on Graviton2. A real-input FFT does half the work of a complex FFT of real samples. In place, rustfft transforms into a scratch buffer and copies the result back; `process` also allocates that buffer on each call.
- Alternatives and tradeoffs: One complex FFT of left + i·right: the same speed, but the mid power of opposite channels and the side power of equal channels are no longer exactly zero. Two FIR blocks in one complex FFT: 0.5% of the set, and a change to the rounding of the render path. f32 transforms: larger changes to every measurement.
- Evidence: The WASM bench record, "Real-input FFT": on Graviton2 the native plan is 7% faster and the set 6% faster; the WASM plan is 7% faster and the render 6% faster.
- Limits and counterexamples: Measurements change at about 1e-14 to 1e-9 relative. One sample of 32 confirmation WAV files changed by one 24-bit step. Equal channels give equal spectra and opposite channels opposite spectra, exactly.
- Verification: `stft` tests compare each channel with `Stft` to 1e-12 of the frame peak and check exact equal and opposite spectra. The suite passes on Graviton2. The eight confirmation sets keep every version, setting and candidate.
- Review condition: A measurement that needs the bits of the complex transform.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D150: Two-phase interpolation and repeated gains

- Recorded: 2026-10-03.
- Origin: User requirement: apply the speed measures of the stem separation to the mastering engine, with identical output.
- Status: Accepted. Implemented in `dsp::Oversampler::at_all`, `dsp::Limiter::push` and `masking::thresholds`.
- Decision: The 4× interpolation computes phases 0 and 1, then phases 2 and 3, in one pass over the samples each. Each phase keeps its 16 lane sums and their order. The limiter keeps the last lookahead reduction and its gain, and computes `gain` only for a new reduction. The masking thresholds take `spreading(z - m)` once for each pair of bands.
- Reason: The interpolation is about 27% of the plan on Graviton2 and loads each sample vector once for each phase. Two phases use 8 accumulator vectors and fit in the 32 NEON registers. The lookahead holds a peak's reduction for as long as the peak is in its window. `spreading` takes an integer difference of bands from −24 to 24, and the thresholds computed it 625 times for each frame.
- Alternatives and tradeoffs: Four phases in one pass: 64 accumulators, 1.7 times slower in WASM on Graviton4. A vector copy of the glibc `pow`: an exhaustive test of f64 inputs is not possible. A cache of the compressor gain: its reduction seldom repeats.
- Evidence: The WASM bench record, "Two-phase interpolation and repeated gains": on Graviton2 the native plan is 11% faster and the set 12% faster. The WASM render on an M1 is 5% faster.
- Limits and counterexamples: The WASM plan on an M1 keeps its time.
- Verification: `dsp::tests::all_phases_equal_each_phase_bit_for_bit` compares `at_all` with `at`. The bench set and the eight confirmation sets give identical WAV files and reports natively. The WASM output of the bench set is identical. In Lambda, D148 to D150 together give a 41% faster planner with identical delivered files.
- Review condition: A change to the tap layout of `dot_pairs`.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D151: One statement of each change in a description

- Recorded: 2026-10-04.
- Origin: User requirement. A Bold description read "More centre presence · More presence. More presence (+2.2 dB at matched level); Less lower mids (-0.6 dB at matched level); Broader stereo balance (+4.0 dB side/mid). -10.7 LUFS. Compared with Light." The user asked for no repetition and no semicolons.
- Status: Accepted. Implemented in `set::describe_measured`, `set::render` and `distributed::finalize_set`.
- Decision: A description states each measured change in one sentence. The two strongest band changes share one sentence, and that sentence names the matched level once. The `set.result-name.v1` name is recorded in the trial as `measured_name`. The description holds the measured changes, the delivered loudness and the comparison base.
- Reason: The result name and the first measured changes come from the same band powers and the same side/mid balance. The description gave one change two or three times.
- Alternatives and tradeoffs: The name as the first sentence of the description: the reader sees the change again. Removal of `set.result-name.v1`: the trial record loses its short name of the result.
- Evidence: The description quoted in "Origin", from an on-device master on 2026-10-04.
- Rule and code references: `set.result-name.v1`; `set::describe_measured`; `set::Trial::measured_name`.
- Limits and counterexamples: A version with no measured change has one sentence that names the comparison base. "Compared with" does not follow that sentence.
- Verification: `tests/set.rs` asserts that a description has no semicolon and names the matched level one time at most. `cargo test --release` passes. The change has no listening result.
- Review condition: A change to the measures of `set.described-duplicate.v1`.
- Work reference: None.
- Supersedes / superseded by: None.

## D152: Delivery ceiling of −1 dBTP

- Recorded: 2026-10-04.
- Origin: User requirement: review the chain against published mastering practice.
- Status: Superseded by D155 on the day of the record. `set::TARGET_CEILING_DBTP` and the ceiling term of `set.source-density.v4` stay.
- Decision: Each set version delivers at −1 dBTP. The crest target of `set.source-density.v4` is 0.5 LU below the v3 target. A source that arrives limited keeps its loudness less 0.5 LU. The clip budgets stay 1.5 dB and 2.5 dB above the ceiling.
- Reason: AES TD1008 recommends a maximum of −1 dBTP at the input of a lossy codec. One streaming platform asks for a master below −1 dBTP. Another recommends 1 dB of headroom or more. D110 set the crest targets with a −0.5 dBTP ceiling. A crest target that moves with the ceiling gives the peak control the same work.
- Alternatives and tradeoffs: A −1 dBTP ceiling with the v3 targets: on four confirmation mixes the maximum limiter reduction increased by up to 0.9 dB and the mean by up to 0.06 dB, or the audibility cap removed 0.5 LU. A −2 dBTP ceiling for a master louder than −14 LUFS, which one streaming platform asks for: each master loses 1 LU more, and the item is open in TODO.md. A ceiling from an encode and decode measurement: the engine has no lossy codec.
- Evidence: The chain practice review, "Ceiling". On the 24 versions of the eight confirmation mixes, the maximum and the mean limiter reduction are equal to the previous figures. The loudness is 0.49 to 0.50 LU lower, from −11.59 to −9.52 LUFS. The peak estimate of each version is −1.0 dBTP.
- Rule and code references: `set.peak-share.v2`; `set.source-density.v4`; `set.audible-loudness.v2`; `set::density`.
- Limits and counterexamples: The peak estimate uses 4× oversampling, and ITU-R BS.1770-5 gives a maximum under-read of 0.55 to 0.69 dB at 4×. The engine does not measure the peak of a decoded lossy file. 43 of 54 chart masters of 2024 reach 0 dBTP or more.
- Verification: `tests/set.rs` asserts `SET_CEILING_DBTP` in each trial. `cargo test --release` passes. The eight confirmation sets rendered on an m8g.4xlarge instance. The change has no listening result.
- Review condition: A true-peak measurement of decoded AAC, Vorbis or Opus files of the set versions. A platform document with a different figure.
- Work reference: TODO.md, "Chain practice review".
- Supersedes / superseded by: Superseded by D155.

## D153: Two conditions for the de-esser key

- Recorded: 2026-10-04.
- Origin: User question: sources state that a de-esser on a full mix lowers the instruments behind the vocal. Experiment on the confirmation mixes and on the same mixes with the vocal removed.
- Status: Accepted. Implemented in `deesser::key` as `set.deesser.v2`.
- Decision: A frame is keyed when the mid channel of the vocal stem has half or more of the 5 to 10 kHz power of the mix's mid channel in that frame (`CARRIED_SHARE`). The stage has no key when the stem has less than 1% of the power of the mix in the active frames of the stem (`VOCAL_SHARE`). The cut, the band and the 20-frame minimum of D133 stay.
- Reason: The vocal stem of a recording without a vocal holds the residue of the separation. `set.deesser.v1` keyed that residue. In a vocal mix, another source carried the band in 6% to 34% of the keyed frames, and the cut lowered that source.
- Alternatives and tradeoffs: Removal of the de-esser from the chain: the sources accept a keyed de-esser in mastering as a repair, and Light has no de-esser. A cut of the vocal stem only: the engine processes the stereo mix (`set.focus-passages.v1`). A higher frame minimum: `track-e` without its vocal keys 1002 frames.
- Evidence: The chain practice review, "De-esser key", from the de-esser key probe. With v1, the eight mixes without their vocals keyed 16 to 1002 frames from a stem 32 to 60 dB below the mix. The sets of four of them had a cut of 4 to 182 frames, by up to 6 dB. With v2, each of the eight keys 0 frames, and the four sets have no de-esser. The eight vocal mixes key 158 to 770 frames with v2 and 169 to 1077 with v1. Their vocal stems are 7.8 to 10.7 dB below the mix.
- Rule and code references: `set.deesser.v2`; `deesser::key`; `deesser::CARRIED_SHARE`; `deesser::VOCAL_SHARE`; `stem_evidence::Evidence`.
- Limits and counterexamples: Each recording without a vocal is a confirmation mix less its vocal stem, not a recording made without a vocal. A vocal more than 20 dB below the mix gets no de-esser. A centered hi-hat in a keyed frame gets the cut of the vocal. No source measures the effect of separation errors on a de-esser.
- Verification: `deesser::tests::a_frame_is_keyed_only_when_the_stem_carries_the_band` and `deesser::tests::the_residue_of_a_separation_keys_no_frame`. `cargo test --release` passes. On the vocal mixes the cut frames of Balanced and Bold decrease by 0% to 9%, and the median cut changes by 0.6 dB at most. The change has no listening result.
- Review condition: A listening comparison of the 6 dB maximum cut with a 3 dB maximum. A separator with a different vocal residue.
- Work reference: TODO.md, "Chain practice review".
- Supersedes / superseded by: Changes the key of D133.

### Updates

- 2026-10-16: The key stays, and the versions of a set have no cut (`set.deesser.v3`, D181). The depth of a cut is the Sibilance step of a request. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D154: Copies of a master at the source rate and at 16 bits

- Recorded: 2026-10-04.
- Origin: User requirement: offer a 16-bit WAV, offer no less than the source, and follow the published delivery practice.
- Status: Accepted. Implemented in `delivery::copy` as `delivery.copy.v1`, in `delivery_copy` of the WASM bridge, and in the `source_sample_rate` field of the source record.
- Decision: The master of each version stays PCM24 at 48 kHz. `delivery::copy` makes a copy at a different sample rate or word length. It converts the rate, measures the peak estimate of the converted audio, turns down a copy above the ceiling by the excess, and dithers at the word length of the copy. The source record gives the sample rate of the supplied file. The web client delivers each file at 48 kHz and uses the function for one 16-bit copy.
- Reason: A source at a different rate has one conversion before the chain. A copy at the rate of a 44.1 kHz source has a second conversion and a trim of up to 0.13 dB. AES TD1008 states that a downward rate conversion can add overshoot. Dither is the last operation on a file.
- Alternatives and tradeoffs: A render of the chain at the source rate: each filter, detector and rule constant has its evidence at 48 kHz. A 16-bit file made by truncation of the PCM24 master: truncation adds distortion that depends on the signal. A copy at a rate above 48 kHz: it adds no content above 24 kHz.
- Evidence: The chain practice review, "Delivery". On the 24 confirmation masters, the 44.1 kHz conversion gave a peak estimate of −1.03 to −0.87 dBTP. The trim was 0 to 0.13 dB, with a median of 0.06 dB.
- Rule and code references: `delivery.copy.v1`; `delivery::copy`; `delivery::Copy`; `audio_io::write_quantized_to`; `resample::convert`.
- Limits and counterexamples: A copy has the 24-bit dither of the master and its own dither. Two streaming platforms ask for the sample rate of the source, and the delivered rate is 48 kHz for each source. The 16-bit copy at 48 kHz is not a CD file. A copy has no decoded-file verification record. The web client makes each copy on the device.
- Verification: `delivery::tests::a_copy_changes_the_rate_and_the_word_length_under_the_ceiling`. `cargo test --release` passes. In the browser, a 44.1 kHz mix gave WAV, FLAC, AIFF and CAF files at 48 kHz and 24 bits, and a 16-bit WAV at 48 kHz.
- Review condition: A render of the chain at 44.1 kHz with the corpus measures. A request for a CD file or for 88.2 or 96 kHz delivery.
- Work reference: TODO.md, "Chain practice review".
- Supersedes / superseded by: None.

### Updates

- 2026-10-07: D173 renders each source at the rate of its family. A master is at 44.1 or 48 kHz, and a copy at the rate of a 44.1 kHz source has no conversion.

## D155: Delivery ceiling of −0.5 dBTP

- Recorded: 2026-10-04.
- Origin: User requirement: the masters are quiet, and a change must not make them quieter.
- Status: Accepted. Implemented in `set::SET_CEILING_DBTP` as `set.peak-share.v3`.
- Decision: Each set version delivers at −0.5 dBTP. `set.source-density.v4` keeps the term that moves the crest target with the ceiling. The term is zero at −0.5 dBTP.
- Reason: D152 made each master 0.5 LU quieter. A −1 dBTP ceiling lowers the decoded peak of a lossy file by 0.5 dB, and the decoded peak is above 0 dBTP at the two ceilings. A commercial service delivers the same mixes at the loudness of the −0.5 dBTP masters, with true peaks above 0 dBTP.
- Alternatives and tradeoffs: A −1 dBTP ceiling with the v3 targets: up to 0.9 dB more maximum limiter reduction, and the audibility cap removes 0.5 LU from some versions (D152). A second file at −1 dBTP for each version: the listener gets two masters of one version.
- Evidence: The chain practice review, "Ceiling". Six masters went through an encode and a decode. At −1 dBTP the decoded true peak is −0.7 to +0.2 dBTP for AAC at 256 kbit/s, 0.0 to +0.6 dBTP for Vorbis at 96 kbit/s and −0.2 to +0.5 dBTP for Opus at 128 kbit/s. At −0.5 dBTP it is −0.2 to +0.7, +0.4 to +1.1 and +0.3 to +0.9 dBTP. The comparison used the files of an online mastering service, eMastered. The files of the online mastering service for seven confirmation mixes have a mean of −10.26 LUFS and true peaks of +0.1 to +1.0 dBTP. Balanced has a mean of −10.24 LUFS on those mixes at −0.5 dBTP and −10.74 LUFS at −1 dBTP.
- Rule and code references: `set.peak-share.v3`; `set.source-density.v4`; `set::SET_CEILING_DBTP`; `set::TARGET_CEILING_DBTP`.
- Limits and counterexamples: AES TD1008 and the published limits of streaming services give −1 dBTP. A player without loudness normalization clips a decoded peak above 0 dBFS. The codec measure uses the ffmpeg AudioToolbox, libvorbis and libopus encoders, two mixes, and a 0.5 dB gain for the −0.5 dBTP level.
- Verification: The eight confirmation sets rendered on an m8g.4xlarge instance. The loudness and the limiter reduction of the 24 versions are equal to the figures before D152. The peak estimate of each version is −0.5 dBTP. The de-esser frames are those of D153. `cargo test --release` passes. The change has no listening result.
- Review condition: A listening comparison of decoded files at the two ceilings without loudness normalization. A platform that refuses a file above −1 dBTP.
- Work reference: TODO.md, "Chain practice review".
- Supersedes / superseded by: Supersedes D152.

### Updates

- 2026-10-16: On the 32 versions of the eight confirmation sets of engine 1.8.1, the 4x estimate reads −0.50 dBTP and a 32x reference reads −0.497 to −0.340 dBTP. On sines, the lowest reading of the 4x estimate equals that of an ideal 4x interpolator to 0.0001 dB. Decoded true peaks (32x reference): AAC at 256 kbit/s (afconvert) −0.23 to +0.58 dBTP, Opus at 128 kbit/s +0.31 to +1.80 dBTP, Opus at 256 kbit/s −0.09 to +0.53 dBTP and MP3 at 320 kbit/s (LAME) −0.26 to +0.56 dBTP.

## D156: Character moves and correction moves

- Recorded: 2026-10-04.
- Origin: User requirement: Bold has less bass than the other versions on track-f near 2:00 (step 1).
- Status: Accepted. Implemented in `broad_tone::Class`, `broad_tone::Move::class`, `broad_tone::is_character` and `set::bold_candidates` as `set.move-class.v1`.
- Decision: Each tone move has a class. The character moves are the lead presence bell, the presence balance bell and the widening. The correction moves are the equal-loudness bass and air shelves, the low-mid cut, the focus bells of the stems, the side cut, the de-esser, the peak-tone restoration and the low-end hold (D159). Vivid multiplies the gain of each character move of the chosen Clear tone by `BOLD_FACTOR` (1.5). Vivid uses the corrections as Clear has them, and it makes its own low-end hold. Natural has no tone moves (D136).
- Reason: The 1.0.0 Bold multiplied the stereo shelves and each mid bell at Q 0.7, so the low-mid cut of track-f went from −1.6 dB to −2.4 dB. A correction repairs a fault of the mix; it has one correct amount for each recording. The lead presence bell is a character move: the plan keeps the presence step of Vivid, and without that bell the step would come only from the presence balance bell and the widening.
- Alternatives and tradeoffs: The lead presence bell as a correction: the presence step from Clear to Vivid becomes smaller on each mix with a vocal lead.
- Evidence: The version tone run record, "Steps 3 and 4". On track-f, Vivid goes from −1.56 dB to −0.97 dB from 250 to 500 Hz against Natural at matched loudness.
- Rule and code references: `set.move-class.v1`; `broad_tone::is_character` (a `Kind::Peak` at `LEAD_BELL_Q`).
- Limits and counterexamples: `is_character` finds a character move by its filter identity, as D128 did. The low-mid cut and the low-end hold are separate fields of the render spec, so they cannot match it.
- Verification: `broad_tone::tests::lead_bells_go_on_the_mid_channel_and_shelves_on_the_mix` and `set::foundation_plan_tests::vivid_scales_character_moves_and_keeps_corrections`. `cargo test --release` passes. The change has no listening result.
- Review condition: A new tone move.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: Changes the Bold scaling of D113 and D128.

### Updates

- 2026-10-04: D161 changes the Vivid scaling: the lead bells keep the gains of Clear, and the presence balance bell and a treble shelf give the steps of Vivid.

## D157: Version names Natural, Clear and Vivid

- Recorded: 2026-10-04.
- Origin: User requirement: the listener chose the names.
- Status: Accepted. Implemented in `set::ESSENTIAL_LABEL`, `set::MASTER_LABEL` and `set::BOLD_LABEL`, and in `bitneedle-app/web/js/mastering.mjs` (`VERSION_LABELS`) and `bitneedle-app/web/js/routes/master.mjs`.
- Decision: The versions are Natural (ID `essential`), Clear (`master`) and Vivid (`bold`). The Denser labels are "Denser Natural" and "Denser Clear". The IDs do not change. The app shows the label of a saved set from its version IDs.
- Reason: The names state the sound of each version: Natural is closest to the mix, Clear has more presence and detail, and Vivid has the most presence, treble and width.
- Alternatives and tradeoffs: None; a naming decision.
- Evidence: None.
- Limits and counterexamples: The documents before this entry use Light, Balanced and Bold, or Essential, Master and Master · Bold. The descriptions of a saved set keep the names of the engine that made it.
- Verification: `cargo test --release` passes.
- Review condition: None.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: Supersedes D136. Superseded by D165.

## D158: Low-mid correction keyed by the stems

- Recorded: 2026-10-04.
- Origin: The version tone plan, step 4. The static low-mid bell cut the same amount in each section, also in the sections where the bass carries the track.
- Status: Accepted. Implemented in `low_mid::key`, `low_mid::Key::correction`, `stem_evidence::Evidence::low_mid`, `treatment_render::Spec::low_mid` and `broad_tone::Master::low_mid_cut_db` as `set.low-mid.v1`.
- Decision: The low-mid cut of `set.broad-tone.v4` becomes a dynamic band of the mid channel. For each masking frame (`masking::frames`, 2048 points, hop 1024), the masking threshold of the power sum of the other stems (`masking::thresholds`) is compared with the power of the lead stem in the masking bands with centres from 250 to 500 Hz. A frame is keyed when the lead is active (within 25 dB of its 95th percentile frame power) and its power there is under the threshold. The masked depth sets the cut of the frame, up to the cut of the whole-track deficit (at most 2 dB, as before). The cut moves with an attack of 0.1 s and a release of 0.5 s, and an envelope blends the band in (`treatment::EnvelopedBand`). The centre is the log-frequency centroid of the positive mean depths, from 250 to 500 Hz. The Q is 1.4, or more where the lower −3 dB point of the band-pass of that centre and Q is under 200 Hz. The band has no static gate, and the presence balance (D134) does not count it. Clear and Vivid use the same correction.
- Reason: The lead is masked in the low mids in 26% to 51% of its active frames on the mixes with a correction, and in 8% to 13% of them on the live recordings with one. The static bell at 350 Hz and Q 0.7 acted from about 175 to 700 Hz. A band with its lower edge at 200 Hz or higher leaves the bass.
- Alternatives and tradeoffs: `dynamic_after_eq`, as the plan gives: its detector follows the band power of the mix, not the masking of the lead. The correction uses the envelope method of the focus bells (`treatment::EnvelopedBand`) instead.
- Evidence: The version tone run record, "Steps 3 and 4" and "Low-mid key". With step 4, track-f Clear goes from −0.63 dB to −0.38 dB from 120 to 250 Hz against Natural, and Vivid from −0.85 dB to −0.59 dB.
- Rule and code references: `set.low-mid.v1`; `low_mid::REGION_HZ`, `low_mid::MINIMUM_Q`, `low_mid::LOWER_EDGE_HZ`, `low_mid::ACTIVE_DB`, `low_mid::ATTACK_SECONDS`, `low_mid::RELEASE_SECONDS`.
- Limits and counterexamples: The other stems are a power sum, with the tonality of the mix. The attack and the release have no listening result. A recording without usable stems has no low-mid correction.
- Verification: `low_mid::tests::the_band_stays_above_the_bass` and `low_mid::tests::only_frames_where_the_others_mask_the_lead_are_keyed`. `cargo test --release` passes. The change has no listening result.
- Review condition: A listening comparison of the attack and the release. A key from the mix less the lead stem.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: Changes the low-mid move of D113 and D128.

## D159: Low-end hold of Clear and Vivid

- Recorded: 2026-10-04.
- Origin: The version tone plan, step 5. Requirement: in each 15 s section, the bass of each version is within 0.5 dB of Natural at matched loudness.
- Status: Accepted. Implemented in `low_end` (`sections`, `changes`, `intended_db`, `next`, `Hold`), `selection::Gates::low_end`, `selection::ladder` and `treatment_render::Spec::low_end_hold` as `set.low-end.v1`.
- Decision: After the render of a Clear or Vivid candidate that passes its gates, the engine measures each 15 s section from 40 to 120 Hz and from 120 to 250 Hz against the Natural render, at matched integrated loudness, less the predicted change of the equal-loudness bass shelf of the version. A section counts when the band of Natural is within 30 dB of its loudest section. In each section where a band is more than 0.5 dB below:
  1. The low-mid correction (D158) decreases by its predicted share of the deficit.
  2. A bell for each held band (69 Hz at Q 0.87 and 173 Hz at Q 1.33, up to +2 dB) returns the band to −0.25 dB. A bell at 1 kHz (Q 0.67, the body from 500 Hz to 2 kHz) cuts the K-weighted loudness that the bells add. Envelopes apply the bells only in those sections.
  3. After at most two more renders, a candidate that stays below fails, and its ladder continues. Vivid falls back to its tone with the widening of Clear, then to the tone of Clear with the Vivid widening.
- Reason: The presence lift and the widening add loudness, and the versions share one loudness target, so the loudness match lowers the bass of Clear and Vivid. With the bass held and the loudness target fixed, the loudness of the bass lift has to come from a band other than the presence: the body. A hold that adds loudness lowers the presence of the version in each section, and the presence steps between the versions become smaller. One band from 40 to 250 Hz, as the plan gives, has most of its power under 120 Hz, so it does not show the loss of 1.05 dB from 120 to 250 Hz on track-f Bold near 2:00. A shelf at 150 Hz lifts 120 to 250 Hz by 0.10 dB per dB at matched loudness.
- Alternatives and tradeoffs: The +1 dB shelf at 150 Hz of the plan; a shelf at 250 Hz that returns the bands to Natural; bells without the body cut. The version tone run record, "Hold design", gives the result of each. The body cut lowers the body by up to 2 dB in some sections of Vivid, and the presence lift of Vivid over Natural is up to 0.77 dB smaller.
- Evidence: The version tone run record, "Corpus", "Stress set" and "References". Sections of Clear and Vivid with a held band more than 0.5 dB below Natural: 145 of 208 before, 3 of 208 after, on the confirmation mixes; 153 of 426 before, 3 of 426 after, on five stress recordings with a delivered Natural. All 16 confirmation versions are delivered.
- Rule and code references: `set.low-end.v1`; `low_end::BANDS_HZ`, `low_end::LIMIT_DB`, `low_end::TARGET_DB`, `low_end::HOLD_MAXIMUM_DB` (`broad_tone::MAXIMUM_DB`), `low_end::BODY_HZ` (`broad_tone::BODY_HZ`), `low_end::ITERATIONS`; `treatment::MAXIMUM_FILTERS` (12 filters in a domain).
- Limits and counterexamples: The hold has no step for a version above Natural: Clear on stress recording 6 is up to 0.63 dB above. Set render time rises by 18% to 91% on two cores. The body cut has no reference: Clear and Vivid have more presence against the body than the online mastering service, the reference masters and the DAW mastering assistant on the four mixes with the largest holds, and the DAW mastering assistant and the reference masters cut the low mids against the body more than Clear does.
- Verification: `low_end::tests::a_section_below_natural_fails_and_a_matched_one_passes` and `low_end::tests::the_hold_applies_only_in_failing_sections`. `cargo test --release`, `cargo clippy --release --all-targets` and `cargo fmt --check` pass. The Lambda crate builds and the WASM crate checks. The change has no listening result.
- Review condition: A listening comparison of the body cut against a hold that takes its loudness from every band. A version above Natural by more than the limit.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: None.

### Updates

- 2026-10-04: The hold continues beyond the full body cut. A step that needs more than the full body cut lifts the rest with the bells alone, and the loudness match takes that loudness from every band. Each step records its predicted rise, and the next render scales the prediction by the measured rise. Without this, a Vivid whose body cut was full gave no more lift and failed its check. On the 16 sources, each version is delivered (Natural of stress recording 2 and stress recording 1 matches the mix, D140), and the presence lift of Clear and Vivid changes by −0.11 to 0.00 dB against engine 1.1.0. The bass loss run record gives the measurements, and the holds of Natural against the mix that the run tried and did not keep.
- 2026-10-04: The hold envelope has the share of each section at the centre of the section, and a raised cosine joins the centres; the factor of the low-mid correction moves in the same way. Before, a 1 s change at each boundary of the 15 s grid moved the body bell by up to 1.89 dB, at up to about 3 dB/s. Now it moves at up to 0.26 dB/s. The bass of the versions holds as before.
- 2026-10-05: Each render of the hold also takes the next restoration pass, and the hold plans against the shift of that pass (D163). The first render of a held candidate has no restoration pass.
- 2026-10-05: The hold plans toward the bass step of the version (`Spec::bass_step_db`, D164), and its check stays at the floor of this entry. The bells of a version with a bass step go up to 2 dB plus the step.
- 2026-10-16: The hold has no bass step (D182). It counts the static low shelves of a version as intended moves, the bass shelves of `set.bass-version.v4` included, and its bells go up to 2 dB. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- 2026-10-16: `set.low-end.v2` (D183). The hold lifts a held band when the other held band already sits within 0.25 dB of its limit or past it, and that band rises by at most 0.25 dB (`low_end::SPILL_DB`). On track-g, the low-mid widening put 120 to 250 Hz of the last section 1.9 dB above `essential`, and the hold could not lift the bass under it. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- 2026-10-16: `set.low-end.v3` (D185). The body bell is removed. The hold plans each lift on the spectrum after the tone and the widening of the version, as a rise of its own section and a fall of every section by the loudness that the lifts add to the file. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- 2026-10-16: Keep decision for the engine item Low-end hold above `essential`. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16. On the ten sources of the regression set, 2 of 128 sections of `master` and 2 of 128 of `bold` have a held band more than 0.5 dB above `essential`, less the intended move: 120 to 250 Hz of track-h from 2:15 to 2:30 (+1.22 and +1.03 dB) and of track-g from 2:30 to 2:45 (+1.86 and +1.60 dB). In both passages the widened low mids of a side-heavy passage raise the band (D183), and the album masters raise it by +1.63 and +1.70 dB there. The hold has no step for a version above `essential`. The acoustic decisions run record, "Hold above essential".

## D160: Restoration of the tone change of the compressor and of peak control

- Recorded: 2026-10-04.
- Origin: The bass loss run. Natural had 0.42 to 0.73 dB less bass against body than the mix on five of sixteen sources, inside each passage.
- Status: Provisional. Implemented in `treatment_render::Renderer::render` and `treatment_render::RESTORATION_PASSES` as `set.peak-tone-restoration.v3`.
- Decision: The restoration measures the tone change from the prefix before the compressor (after the tone, the width, the de-esser and the dynamic bands) to the render, in each 0.34 s window at the matched K-weighted level of the window. After the restoring render it measures again, and a second pass adds the filters that return the rest. The gains of a band add over the passes, up to ±1.5 dB, and the dead zone of 0.1 dB stays.
- Reason: The compressor, with its detector high-pass at the bass body frequency, and peak control lower the presence transients and the bass, so at the K-weighted level of a passage the body rises by 0.09 to 0.25 dB. The restoration of v2 measured from the compressed prefix and kept the change of the compressor. One pass returned about half of the measured change.
- Alternatives and tradeoffs: A detector high-pass of 250 Hz: 10% to 64% less loss in the windows and up to 0.60 dB more P90 attack loss. A hold of Natural against the mix at matched loudness of the whole file: it follows the dynamics of the chain, and on stress recording 6 it found section losses of 4 to 7 dB. A smaller dead zone.
- Evidence: The bass loss run record, "Restoration v3". The balance of bass against body of Natural moves by 0.06 to 0.26 dB toward the mix on the five mixes, and by 0.05 dB or less on the other three confirmation mixes.
- Rule and code references: `set.peak-tone-restoration.v3`; `treatment_render::RESTORATION_PASSES`, `treatment_render::Restoration::renders`; the bass loss probe, the tone measures probe.
- Limits and counterexamples: A version renders up to three times; a set on two cores takes −12% to +18% of the time of v2. The body stays 0.07 to 0.18 dB above the mix on four mixes. The change has no listening result.
- Verification: `restoration_reports_the_local_rule_and_does_not_widen_the_change` in `tests/treatment_render.rs`. `cargo test --release` passes.
- Review condition: A listening comparison of Natural and the mix on track-a and track-g.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: Supersedes `set.peak-tone-restoration.v2` (D117, D120). Superseded by `set.peak-tone-restoration.v4` (D163).

### Updates

- 2026-10-05: `set.peak-tone-restoration.v4` (D163) keeps the measure, the passes, the limit and the dead zone. The restoring gains are a field of the spec, a version starts from the gains of Natural, and a held version takes its passes in the renders of the hold.

## D161: Presence and treble steps of Vivid

- Recorded: 2026-10-04.
- Origin: User requirement: three distinct, strong versions that work on almost any track. The version steps plan.
- Status: Provisional. Implemented in `broad_tone::vivid_tone` and `set::bold_candidates` as `set.vivid-steps.v1`.
- Decision: The lead bells of Vivid keep the gains of Clear. The presence balance bell of Vivid (stereo, 3162 Hz, Q 0.7) has the larger of 1.5 times the gain of Clear and the gain that puts the predicted presence against body (2 to 5 kHz against 500 Hz to 2 kHz) 1 dB above Clear, at most 6 dB. Where the 6 to 12 kHz band is material, a high shelf at 6 kHz puts the predicted treble against body 0.5 dB above Clear, at most 2 dB. The predictions use the source spectrum and the side gain of Clear for both tones, so the tone gives the presence and treble steps and the widening gives the width step.
- Reason: With the whole tone of Clear multiplied by 1.5 (D156), the presence step from Clear to Vivid was 0.28 to 1.03 dB, and it depended on the size of the moves of Clear. The lead bell on the mid channel raised the mid against the side, so Vivid was narrower than Clear on four mixes. No move lifted the treble.
- Alternatives and tradeoffs: A minimum step of 0.5 dB, the duplicate threshold: a step at the edge of a duplicate. A treble step from an air shelf above 8 kHz: codec low-passes of the phone recordings. The prediction with the Vivid widening: on stress recording 5 the widening gave most of the predicted step, and Vivid was a duplicate.
- Evidence: The version steps run record. On the 16 sources, the presence step is 0.66 to 1.31 dB at matched loudness and 1.35 to 2.17 dB in presence against body on the confirmation mixes. Vivid has more treble on each source and more width on 15 of 16.
- Rule and code references: `set.vivid-steps.v1`; `broad_tone::VIVID_PRESENCE_STEP_DB`, `broad_tone::VIVID_TREBLE_STEP_DB`, `broad_tone::VIVID_TREBLE_SHELF_HZ`, `broad_tone::VIVID_PRESENCE_MAXIMUM_DB`.
- Limits and counterexamples: Vivid has more presence against body than the online mastering service, the reference masters and the DAW mastering assistant on each confirmation mix. Clear and Vivid lower air against presence by 1.1 to 3.2 dB, where the online mastering service raises it. Vivid of stress recording 3 is 0.16 dB narrower than Clear. The treble step is 0.29 dB or less where the 6 to 12 kHz band is not material. The change has no listening result.
- Verification: `set::foundation_plan_tests::vivid_steps_presence_and_treble_and_keeps_corrections`. `cargo test --release` passes.
- Review condition: A listening comparison of Vivid on track-f and track-e. A measure of the width step of Vivid.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: Changes the Vivid scaling of D156: the lead bells are no longer scaled.

### Updates

- 2026-10-05: A presence step of 1.5 dB raised the median presence step from `essential` from 2.79 to 3.20 dB, and a width step of 0.5 dB raised the median side/mid step from 1.59 to 1.89 dB. Vivid was already 1.34 to 2.09 dB brighter in presence against body than the brightest reference on six confirmation mixes, and each arm made it brighter. The step stays at 1 dB.
- 2026-10-16: `set.vivid-steps.v2` (D180). The presence step stays 1 dB, and the whole tone of `bold` with its widening stays within the presence bound of `master` plus 0.48 dB, the P90 side of the album pairs. The floor of 1.5 times the gain of `master` is removed. The treble step stays. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D162: Air balance of Clear and Vivid

- Recorded: 2026-10-04.
- Origin: User requirement: a treble and air rule. Clear was darker than Natural from 6 to 12 kHz on three mixes, and Clear and Vivid lowered air against presence by 1.1 to 3.2 dB.
- Status: Provisional. Implemented in `broad_tone::Move::AirBalance`, `broad_tone::air_gain` and `broad_tone::vivid_tone` as `set.air-balance.v1`, in `set.broad-tone.v5`.
- Decision: After the presence moves of Clear, a high shelf at 8 kHz returns the predicted air (8 to 16 kHz) against presence (2 to 5 kHz) of the tone to that of the mix, from the source spectrum, with at most 2 dB. It is proposed from 0.5 dB. It is a character move. Vivid solves the shelf again for its own presence. The air shelf and the treble shelf of D161 have no gate; the air shelf needs a source band to 16 kHz and the treble shelf a band to 12 kHz, from the codec bandwidth of the source condition (D142).
- Reason: The presence moves raise the presence against the rest; at matched loudness the air falls against it, and the 6 to 12 kHz band of Clear fell under Natural. The mix sets its own balance of air against presence. The 12 to 20 kHz band of a full-range mix holds 0.14% to 0.53% of the power of its strongest band, under the 1% of a material band (D139), so a gate on that band removed the move from each mix (D144).
- Alternatives and tradeoffs: Air against presence toward a balance of equal loudness per band: like the bass, each source would get the largest shelf. Air toward the files of the online mastering service: a reference is not a target (D112). The gate on 12 to 20 kHz: no move on any mix.
- Evidence: The version steps run record, "Air balance". Clear is +0.28 to +1.47 dB against Natural from 6 to 12 kHz on each confirmation mix. Air against presence of Clear is −0.86 to +0.09 dB against the mix, and of Vivid −1.22 to −0.14 dB.
- Rule and code references: `set.air-balance.v1`; `broad_tone::AIR_HZ`, `broad_tone::AIR_SHELF_HZ`, `broad_tone::air_shelf`.
- Limits and counterexamples: The shelf reaches 2 dB on five of eight mixes. The limiter maximum reduction rises by up to 0.46 dB, and the de-esser cuts up to 80 more frames with a median up to 0.7 dB deeper. The change has no listening result.
- Verification: `broad_tone::tests::the_air_follows_the_presence_lift`. `cargo test --release` passes.
- Review condition: A listening comparison of the air shelf on track-e and track-f. A source with harsh cymbals.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: Changes the gate of the treble shelf of D161.

### Updates

- 2026-10-16: The air balance stays (D181). With the presence bound of D180, the shelf of `master` is 1.24 to 2.0 dB on seven album tracks and absent on study track 1. Air − presence of `master` is +0.15 to +0.80 dB against the mix (median +0.46 dB), and of `bold` −0.60 to +0.92 dB; the album masters move it by −0.28 to +0.09 dB from P25 to P75. On study track 1, with no air shelf and no presence move, `master` measures +0.15 dB. The body bell of the low-end hold (Q 0.67 at 1 kHz) also lowers the lower presence region. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D163: Render work of Clear and Vivid

- Recorded: 2026-10-05.
- Origin: User requirement: render the cloud versions at the speed of engine 1.0.0 or faster. With engine 1.1.0, the Clear and Vivid families of track-f took 2.9 times as long.
- Status: Provisional. Implemented in `resample::convert`, `stem_evidence`, `deesser::key_with_frames`, `refinement::analyze_matched`, `treatment_render::Spec::restoration_db`, `treatment_render::Renderer::render_passes`, `treatment_render::Renderer::restore`, `treatment_render::next_gains`, `selection::hold_low_end`, `selection::ladder` and `render_blocks::segmented_min_frames`, as `set.peak-tone-restoration.v4`.
- Decision: The resampler fills contiguous parts of its output on the available cores. The planner resamples the vocal stem once, and its masking frames serve the low-mid key and the de-esser key. The restoring gains are a field of the spec, and they apply after the window measure and before the compressor. A version starts from the restoring gains of its reference render. The first render of a candidate with the low-end gate has no restoration pass; if it fails a gate, its restoration completes and the gates decide again. Each later render takes the next hold step and the next restoration pass, up to `low_end::ITERATIONS` steps and `RESTORATION_PASSES` passes. The hold plans against its changes plus the shift that the restoration pass causes in each held band at matched loudness, from the source spectrum. Peak control renders in 60 s segments for each recording longer than one segment.
- Reason: In 1.1.0, the first render and each of the two hold steps of track-f Clear took one render and two restoration passes: 11 prefixes and 23 probes, against 4 and 8 in 1.0.0. The planner resampled each stem twice on one core. Peak control of a recording of five minutes or less ran on one core, for 24.6% of the time of the Clear family.
- Alternatives and tradeoffs: The restoration passes in the first render before the hold (form 1): 7 prefixes and 16 probes for track-f Clear. Shared renders without the planned shift (form 2): on track-a, 40–120 Hz of Vivid went from −0.17 to −0.34 dB against Natural. A drive slope from the search before: the first probe misses by up to 0.28 LU, so the search still takes two probes. Hold bells up to +3 dB: track-c delivers Vivid, and its Clear passes its first candidate, 2.4 dB narrower.
- Evidence: The family speed run record. Planner and slowest family on two cores: track-f 60.1 s (1.0.0: 63.4 s, 1.1.0: 135.2 s), track-b 47.6 s (1.0.0: 47.2 s), stress recording 1 134.0 s (1.0.0: 136.8 s).
- Rule and code references: `set.peak-tone-restoration.v4`, `set.low-end.v1`; `selection::hold_low_end`.
- Limits and counterexamples: track-c delivers no Vivid. Its best Vivid candidate holds −0.51 dB against the limit of −0.5 dB; with 1.1.0 it held −0.46 dB. On track-f, one bass band of Clear and of Vivid is 0.06 to 0.09 dB lower than with 1.1.0. The Clear family of stress recording 1 takes 15 s more than with 1.0.0. Segmented and whole renders differ in the last bit of some samples (D145). The change has no listening result.
- Verification: The plans of track-f and stress recording 1, and the Natural WAVs of both, are identical with and without changes 1 to 3 of the family speed run. On the 16 sources, each measure of each version is within 0.12 dB of 1.1.0. `cargo test --release`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo fmt --check` pass.
- Review condition: A Lambda timing of the deployed families. A rule for a section that the largest hold bell cannot return.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: Supersedes `set.peak-tone-restoration.v3` (D160). Changes the segment threshold of D145 and the renders of D159.

### Updates

- 2026-10-05: Engine 1.2.0 in Lambda, on the 3.6-minute track-h: planner 25.6 s, families 11.4, 42.9 and 47.0 s, finalizer 12.2 s, stems 53.8 s; peak memory 1,203 MB of 3,008 MB.
- 2026-10-16: A render repeats for the restoration only when a band change exceeds the dead zone and the next gains move a filter by `RESTORATION_FILTER_FLOOR_DB` or more (`treatment_render::next_gains`). On the nine sets of the regression set, 12 of 36 delivered renders take one render, six of them with a band change just over the dead zone. Each repeat without a band change over the dead zone is a step of the low-end hold. Of the 70 prepared prefixes, 11 are restoration passes that change only the restoring gains. A cache of the shaping before the restoring filters can save at most the shaping of those 11 prefixes, about 3% of the time of a set; the engine keeps the shaping of each pass.

## D164: Bass version

- Recorded: 2026-10-05.
- Origin: User requirement: a master with more bass in the set when the track has bass, inside the three versions.
- Status: Provisional. Implemented in `set::BASS_STEP_DB`, `set::BASS_SHARE`, `set::bass_candidates`, `set::WorkerVersion::bass_step_db`, `stem_evidence::Evidence::rhythm_low_share`, `treatment_render::Spec::bass_step_db`, `selection::hold_low_end`, `low_end::bell_maximum_db` and `distributed::render_family`, as `set.bass-version.v1`.
- Decision: When the drums and bass stems hold more than half of the stem power from 40 to 120 Hz, the `master` version has a bass step of 1.5 dB. The low-end hold plans each section of each held band toward 1.25 dB above `essential` at matched loudness, with bells of up to 3.5 dB and the body cut of D159. The check of the hold stays at the floor of D159, so a version that reaches less than its step is delivered. The tone of the candidates stays. A `bold` version builds from the planned candidate without the step.
- Reason: The low-end hold lifts the bass of each section and pays for its loudness with a body cut. The stems show which tracks have a bass part: the drums and bass stems hold 0.96 to 1.00 of the low band on 13 sources and 0.00 to 0.37 on three phone and camera recordings.
- Alternatives and tradeoffs: A stereo low shelf of 2 or 3 dB at 80 or 120 Hz: 40–120 Hz rose 0.46 to 1.51 dB, and 120–250 Hz fell 0.34 to 0.80 dB under `essential`. A hold target without the presence balance bell: presence fell 1.05 dB under `essential` on track-e. A step of 0.5 dB: 40–250 Hz rose 0.23 to 0.44 dB. A hold check at the step: the version was lost on four of eight mixes. Bells of at most 2 dB: on four of eight mixes the bells reached the limit in each section, and track-f rose 0.42 dB.
- Evidence: The version character run record. On the 13 sources with bass, 40–250 Hz of `master` is 0.67 to 1.35 dB above `essential`, and the lowest section of a held band is −0.29 to +1.08 dB.
- Rule and code references: `set.bass-version.v1`, `set.low-end.v1`.
- Limits and counterexamples: The P90 attack loss of `master` is 0.20 to 0.91 dB larger than that of the Clear of D163. At matched loudness, 2–5 kHz of `master` is −0.77 to +0.54 dB and 6–12 kHz −1.10 to +0.14 dB against `essential`. A bass in the `other` stem does not count. The change has no listening result.
- Verification: `cargo test --release` passes.
- Review condition: A listening comparison of the bass version on track-f and track-g. A track whose bass part is in the `other` stem.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: Extends D159.

### Updates

- 2026-10-05: Four arms for the attack loss and the top end of the bass version did not meet their criteria: a compressor threshold raised by the detector lift of the step, the bells after the compressor, a body cut of up to 3 dB, and the first and third together. The attack loss stayed 0.20 to 1.10 dB above Clear. B1 and B2 changed it by −0.10 to +0.24 dB against 1.2.0, so the lift of the bass causes it.
- 2026-10-11: The attack loss against the mix counts the tone of the bass version. The tone prefix of the version (the mix with its tone, width, de-esser, low-mid correction and hold, without the compressor) measures 0.51 to 2.06 dB more P90 attack loss against the mix than the tone prefix of Clear, more than the whole difference between the delivered versions. Against its own tone prefix, the bass version loses −0.25 to +0.39 dB more onset energy (P90, D126) than Clear through the compressor and peak control: within 0.3 dB on 12 of 13 sources. The peak control of the bass version costs 0.77 dB (median) of maximum limiter reduction against Clear, more than 1 dB on track-a, track-c and stress recording 5, and its audibility point (D131) is 0.66 LU (median) under that of Clear. The bass version is within 0.5 LU of its point on stress recording 2 (0.07 LU) and stress recording 1 (0.24 LU). The limit entry of this decision stands as measured; the criterion of 0.3 dB over Clear needs the measure against the tone prefix.
- 2026-10-11: Each render reports the P90 loss of onset energy against the prefix before its compressor (`Rendered::energy_attack_loss_p90_db`), from the onsets and blocks that the shaping pass measures before the compressor runs, and each candidate trial carries it beside the loss against the mix (`CandidateTrial::energy_attack_loss_p90_db`). No gate uses it. A span render (a preview) has none.
- 2026-10-14: `set.bass-version.v2`. On the early track-b mix (stems estimated locally), the equal-loudness rule proposed a bass cut of −1.03 dB at 150 Hz and the stems gave the bass step. The tonal gate of the cut measured each candidate's band under 120 Hz against the body, found it 1.8 to 2.3 dB up, and refused all 45 candidates, so no `master` and, since `bold` derives from the chosen `master` candidate, no `bold`. The gate verified one move alone in a band two rules move. The user asked whether withholding the step when the rule proposes a cut is a fix or a special case; it is a precedence rule, so the change is in the gate instead: `set::bass_candidates` nets each tonal gate on a band under 500 Hz that the hold also lifts, the gate's planned gain (`TonalGate::planned_db`) plus the hold's step less `low_end::TARGET_DB`, 1.25 dB; the direction and the minimum come from the sum, the prediction from the tones adds the hold (`TonalGate::hold_db`), and a sum under `broad_tone::MINIMUM_DB` leaves the gate out, since the low-end gate already checks the hold net of the equal-loudness bass move. The eight album mixes proposed no bass cut, so their sets are unchanged. On the early mix `master` now delivers from its first candidate; `bold` fails the low-end gate by 0.5 dB on every candidate (the worst 15 s section of a held band ends 0.95 to 1.01 dB under Essential beyond the cut), as the 13 March track-b mix did. `set::tests::the_bass_step_nets_a_bass_cut_into_its_gate`.
- 2026-10-16: `set.bass-version.v3`, the failure. Engine 1.8.0 delivered only `essential` and `open` for regression mix 2 (2:31, 44.1 kHz, 16-bit), in the cloud and in the native set. The equal-loudness rule proposed a bass cut of −0.65 dB, and v2 netted its gate to a lift of +0.60 dB. The selection checks the tonal gates on the render before the hold, and the hold runs only after they pass. The prediction refused candidates 0 and 1 (−0.45 and −0.14 dB against −0.10 dB). Candidates 2 to 4 measured −1.13, −0.80 and −0.34 dB against +0.20 dB, with none of the lift. In the cloud, the speculative `bold` passed its gates, and the finalizer refused it because `master` chose no candidate.
- 2026-10-16: The gate in a held band. In the bands that the hold moves, the hold sets the move of the version (`low_end::lifts`: under 120 Hz and 120 to 500 Hz). It plans each failing section toward the step plus `low_end::TARGET_DB` above `essential`, net of the equal-loudness bass move. The low-end gate checks each section at the floor of D159. `set::bass_candidates` leaves out each tonal gate on such a band that measures against an unmoved band or the whole file. The change removes `TonalGate::planned_db` and `TonalGate::hold_db`, and the prediction of a gate is again the change from the tones.
- 2026-10-16: Reason for the gate rule. A tonal gate on a held band verifies a planned move that the hold replaces. Before the hold, the gate measures the tone without the lift. After the hold, it would check a share of the step, and this decision keeps the check of the hold at the floor. Where v2 netted the gate to a lift, the gate could pass only when the tone itself lifted the band.
- 2026-10-16: The fallback without the step. When no candidate with the step passes, `master` renders the same candidates without the step (`set::base_ladder`, in `set::render` and `distributed::render_family_in`). The version is then the plain `master`, with the label Forward and a basis that names `set.bass-version.v3`. An index names the same planned candidate in both ladders, as `unless_base_candidate` and `SpeculativeBase::candidate` do. So the speculative `bold` stands when `master` chooses candidate 0, with or without the step. Before this change, a bass version had no fallback, and the set lost `master` and `bold` together.
- 2026-10-16: `bold` without a chosen `master`. When `master` chooses no candidate, the finalizer keeps the speculative `bold` that passed its gates (D147). The native set renders `bold` from the planned first `master` candidate without the step, the spec of the speculative `bold`. The basis of that `bold` names `set.bass-version.v3`. Reason: `bold` needs only that planned spec, and its gates compare it with `essential`, not with `master`.
- 2026-10-16: Evidence. With v3, regression mix 2 delivers `essential`, `master` (Full, candidate 0 with the step), `bold` and `open`, in the native set and on the local cloud path. In each path, the `essential` and `open` WAVs are identical to those of 1.8.0. Full against `essential` at matched loudness: 40–250 Hz +0.96 dB, 500 Hz–2 kHz −1.98 dB (the body cut of the hold), 6–12 kHz −1.90 dB, side/mid −1.58 dB. Variant A of the run, the gate of v2 with the fallback, delivers the plain `master` from candidate 0 in both paths. Variant B, the gate of v2 without the fallback, delivers `bold` and no `master` in both paths. The eight album sets with stems and the early track-b set are identical to those of 1.8.0.
- 2026-10-16: Limits and verification. In a bass version, no tonal gate checks the equal-loudness bass move; the floor of the low-end gate checks the held bands. A `master` that falls back renders up to two ladders, and each can render again with the instant attack. On regression mix 2, 6–12 kHz of Full is −1.90 dB against `essential`, beyond the range of −1.10 to +0.14 dB of this decision. The change has no listening result. Tests: `set::foundation_plan_tests::the_bass_step_leaves_out_the_gates_of_the_held_bands`, and `a_bass_version_whose_bass_candidates_fail_renders_its_candidates_without_the_step` and `the_finalizer_delivers_the_speculative_bold_when_master_chose_no_candidate` in `tests/set.rs`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass.
- 2026-10-16: `set.bass-version.v4` (D182) replaces the bass step with the two low shelves of `open::plan`, the median low-end moves of the album pairs. The hold has no step. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- 2026-10-16: Family time. On a 10.9-minute file with two threads, the planned `master` took 159.0 CPU-s on an M1. A `master` whose candidates fail in each ladder (a limiter-maximum gate of −100 dB on each candidate) took 924.6 CPU-s. Lambda took 0.9 to 1.9 s for each CPU-second of the M1 on the families of regression mix 2. At 29.9 minutes the fallback takes about 2,290 to 4,830 s against the Lambda limit of 900 s.

## D165: Version names from the measured sound

- Recorded: 2026-10-05.
- Origin: User requirement: names that do not rank the versions. "Calling one polished implies the other two are not"; "even Natural is bad". The names and the descriptions come from the track and from what the engine did to it.
- Status: Provisional. Implemented in `version_names`, `set::Sink::name`, the native and WASM sinks and `distributed::finalize_set`, as `set.version-names.v1`.
- Decision: After the last version, each delivered version gets a name and a description from its measured sound at matched loudness: the six bands of `stereo::BAND_EDGES_HZ`, the bass of the held bands (`low_end::bass_level_db`) and the width. The names are Warm, Smooth, Full, Forward, Bright, Airy, Wide, Focused, Vivid and Essential. A later version takes the name of its largest step from the version before it. A version keeps the name that its rule gives it, Full for the bass version (D164), when that step is 0.5 dB or more. The version closest to the mix takes the name of its largest margin over every other version, or Essential. A name needs 0.5 dB, and each name occurs once in a set. The description states the character in plain words, and the measured figures move to `detail`. Until the names come, the planned labels are Essential, Forward (Full for a bass version) and Vivid.
- Reason: Natural, Clear and Polished compare the versions by quality, and each version is a complete master. A name from the measured sound is true for that track.
- Alternatives and tradeoffs: Fixed names. The name of the version closest to the mix from the mix against the equal-loudness reference: 13 of 16 mixes lie under the presence balance of D134, so most would be Warm. Forward as presence against body: the body cut of the low-end hold raises it, to 3.01 dB on track-f where presence rose 0.89 dB. Names from the dynamics: the crest and the attack loss of the versions differ by 0.33 dB or less.
- Evidence: The version character run record.
- Rule and code references: `set.version-names.v1`; `version_names::NAMES`.
- Limits and counterexamples: The third version takes Forward on most sources. A step near 0.5 dB can change the name between two renders of a similar mix. The change has no listening result.
- Verification: `version_names::tests`, and `tests/set.rs`, which checks that the cloud finalizer and the native set give the same names and descriptions. `cargo test --release` passes.
- Review condition: A user test of the names on the MASTER screen.
- Work reference: TODO.md, "Version tone".
- Supersedes / superseded by: Supersedes the names of D157.

### Updates

- 2026-10-08: A later version took any free name when its largest step was in a character that a version before it held, and was described as "A different balance from the version before." On the clip from the user's video, Full had 0.13 dB less bass and 1.1 dB more presence than Forward. Such a version now takes the comparative name (More forward: "Brings the vocal and lead further forward than Forward."). A later version with no step of 0.5 dB or more in any character over the version before it is not delivered (status `not-distinct`), as no name would tell a listener what it changes. User decision of 2026-10-08: fix the name in the engine or do not provide the version.

## D166: Limiter attack and local true-peak correction

- Recorded: 2026-10-05.
- Origin: Engineering proposal from the comparison with a commercial limiter, Sonnox Oxford Limiter, and the open attack loss of the Full master.
- Status: Accepted. Released in engine 1.3.0 on 2026-10-05.
- Implementation: Experimental on the branch `peak-attack` (`6ef88c0`). `LimiterSettings::attack_ms`, `Spec::true_peak`, `Oversampler::lower_peaks`, and the rule `set.peak-attack.v1` (`set::peak_attack`, `set::TRUE_PEAK`, `set::PEAK_ATTACK_MS`).
- Decision or proposal: Measure an attack time of the limiter (0.3, 1 and 3 ms), a clip budget of 3 dB, and a local true-peak correction, in one bundle run on the 16 version sources. The criteria are in the peak attack plan.
- Reason: The commercial limiter lets transients pass a slower attack and controls them in a later stage. Its true-peak stage corrects a true peak only for the duration of the error. The engine limiter applies each new maximum at once, and its true-peak trim lowers the whole render.
- Alternatives and tradeoffs: A separate clipper curve is not measured. A tanh curve with the same knee start differs from the quadratic knee by at most 0.07 dB at a 1.5 dB budget and 0.16 dB at 3 dB.
- Evidence: The peak attack plan.
- Rule and code references: `dsp::Limiter::push`, `render_blocks::fit_true_peak`, the peak report probe.
- Limits and counterexamples: The masking model is not calibrated against listening tests on this material (D127).
- Verification: `cargo test --release` passes on the server, with tests of the attack, of `lower_peaks` and of the local mode in a render.
- Review condition: The results of the run.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

### Updates

- 2026-10-05: Iteration 1. No arm meets the criteria. The 1.2.0 trim lowers 37 of 47 versions as a whole (median 0.55 dB). The local correction (arm T) lowers the masked NMR by a median of 1.59 dB and raises it on no version. A 3 ms attack lowers it on each version of the studio mixes and raises it by up to 1.79 dB on live recordings. A 3 dB clip budget raises it by up to 4.77 dB. Most of the attack difference between Full and Clear comes before peak control.
- 2026-10-05: The user stated that live sets are a low priority and can have their own path. Rule `set.peak-attack.v1`: the local correction for each version; the instant attack for a captured recording; for another source, the 3 ms attack when its base render meets the NMR bound of 1.2.0 and keeps more onset energy.
- 2026-10-05: Iteration 4, after the user's decisions (the 3 ms attack; the planner time is acceptable; live sets are mono and take their own path; optimise for studio masters): mono sources keep the instant attack, and the fallback renders the reference again. On the 8 studio mixes the four conditions pass; on the stress set no version is delivered less or has a higher NMR. The suite passes. The release check delivers the same versions on 16 sources and the same names on 15.
- 2026-10-05: Iteration 2 compared the renders at the same drive and gave the attack to 3 of 8 studio mixes; condition 4 failed by one section at −0.505 dB. Iteration 3 compares at matched loudness: the attack goes to the 8 studio mixes, and conditions 1 to 4 pass. On the studio mixes the NMR falls by 1.11 to 6.80 dB and the median energy attack loss P90 from 2.90 to 0.78 dB. Condition 5 fails: the planner of track-h takes 12.5% more time. The set test loses Master on its synthetic 120 Hz pulses with the 3 ms attack, and passes with 1 ms.

## D167: Program-dependent release and a parallel shaper rejected

- Recorded: 2026-10-05.
- Origin: Engineering proposal from a review of two limiters and two saturation plugins: LoudMax, W1 Limiter, Oxford Inflator and JS Inflator.
- Status: Rejected. Engine 1.3.0 is unchanged. The branch `shape-release` holds the shaper switch.
- Implementation: Absent from `main`.
- Decision or proposal: Keep the 10 ms release without the sustained stage, and add no shaper before the limiter.
- Reason: On the 8 studio mixes, a sustained release of 100 to 300 ms changes the masked NMR by 0.12 dB or less and the onset energy by 0.07 dB or less: with the 3 ms attack of 1.3.0, the clipper takes most of the overs. A parallel cubic shaper changes the median NMR by −0.09 dB at 10% and raises it by up to 3.64 dB at 25% and 50%. Above 240 Hz it raises it by 1.76 to 14.98 dB.
- Alternatives and tradeoffs: The two limiters of the review have a fixed attack and no oversampled true-peak control; the review takes nothing else from them. A three-band shaper is not measured.
- Evidence: The shape release plan, its run record; its result files.
- Rule and code references: `dsp::Limiter` (sustained stage); `dsp::Shaper` on the branch `shape-release`.
- Limits and counterexamples: The masking model is not calibrated against listening tests on this material (D127). The measure counts a linear lift of a band as a change from the prefix.
- Verification: `cargo test --release` passes on the branch. No listening test.
- Review condition: A limiter attack shorter than 3 ms, where the limiter takes more of the overs.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D168: Peak control stays after the commercial limiter comparison

- Recorded: 2026-10-06.
- Origin: Experiment: a commercial limiter, FabFilter Pro-L 2, as a black box.
- Status: Accepted. Engine 1.3.0 is unchanged.
- Implementation: Absent. A plug-in render tool and a peak pair probe measure external renders.
- Decision or proposal: Keep the peak control of 1.3.0. Do not rebuild the limiter on the two-stage design of the commercial limiter.
- Reason: Seven of the eight styles of the commercial limiter have a lower masked NMR than 1.3.0 on each studio mix, by 2.8 to 3.6 dB at the median, with an energy attack loss within 0.2 dB. In a matched-loudness listening test of the Full master of the 8 mixes, the user could barely hear a difference and preferred 1.3.0 slightly if at all. On track-c, where 1.3.0 has an NMR of −0.56 dB against −4.59 dB with one of the styles, the user preferred 1.3.0 and rated a second style the weakest.
- Alternatives and tradeoffs: A slow gain envelope under a fast stage with a lookahead of about 0.2 ms, as the sweep of the commercial limiter shows. A clip-led third version as character.
- Evidence: The limiter comparison record; its result files.
- Rule and code references: the plug-in render tool, the peak pair probe.
- Limits and counterexamples: One listener and one version. The masking model ranks distortion under the masking threshold, where a difference of 3 dB was not audible; near the threshold, the preferred master had the higher NMR.
- Verification: Listening test by the user, 2026-10-06.
- Review condition: A master with a masked NMR of peak control above about −5 dB, or a listening result that hears peak control.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D169: Compressor stays after the commercial compressor comparison

- Recorded: 2026-10-06.
- Origin: Experiment: a commercial compressor, FabFilter Pro-C 3, in place of the engine compressor.
- Status: Accepted. Engine 1.3.0 is unchanged.
- Implementation: Absent. A chain render probe and the dump of the signal before the compressor support the study.
- Decision or proposal: Keep `set.compression.v2`.
- Reason: At the settings and the mean gain reduction of the engine compressor, each of the eight styles of the commercial compressor loses more onset energy, by 0.12 dB (its mastering style) to 1.01 dB (its bus style) at the median. In a matched-loudness listening test the user found the engine compressor, the mastering style and a third style very close, preferred the engine compressor and did not like the third style.
- Alternatives and tradeoffs: The mastering style lowers 2–10 kHz after a hit on 3.2% of frames, against 12.1% for the engine compressor, at a small cost in onset energy.
- Evidence: The compressor comparison record; its result files.
- Rule and code references: `compressor::process`, the chain render probe.
- Limits and counterexamples: One listener; the Full version of 8 studio mixes; the commercial compressor at the time constants of the engine, not at its own defaults.
- Verification: Listening test by the user, 2026-10-06.
- Review condition: A vocal-led master with a heavy kick where the vocal dips after each hit.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D170: De-esser stays after the commercial de-esser comparison

- Recorded: 2026-10-06.
- Origin: Experiment: a commercial de-esser, FabFilter Pro-DS, in place of the engine de-esser.
- Status: Accepted. Engine 1.3.0 is unchanged.
- Implementation: Absent. With `MASTER_PREFIX_WAV`, a family also writes the signal before the compressor without the de-esser.
- Decision or proposal: Keep `set.deesser.v2`.
- Reason: At the same mean cut of the sibilant band in the keyed frames, the commercial de-esser in its full-mix mode cuts the sibilant band in the other frames by 2.0 to 2.3 dB at the median, against 0.01 dB, and lowers 1–4 kHz in the keyed frames by 0.8 to 1.0 dB, against 0.00 dB. The share of keyed frames that stay sibilant is the same. On track-a, the mix with the largest differences, the user preferred the engine de-esser, with a small difference.
- Alternatives and tradeoffs: The commercial de-esser with the engine band, and with its defaults. Its single-vocal mode makes no cut on a full mix.
- Evidence: the comparison record; the comparison results.
- Rule and code references: `deesser::key`, `deesser::Plan::apply`.
- Limits and counterexamples: One listener and one mix in the listening test. The keyed frames come from the engine key, and the vocal stem keys only the engine de-esser.
- Verification: Listening test by the user, 2026-10-06.
- Review condition: A vocal with sibilance that the stem key misses, or a mix without a usable vocal stem.
- Work reference: TODO.md, "Human-level program".
- Supersedes / superseded by: None.

## D171: Controls of a pro account on a delivered version

- Recorded: 2026-10-06.
- Origin: User requirement: a paid tier where the listener changes the settings of a master, with quick changes and the existing analysis. User decisions of 2026-10-06: the final render solves the low-end hold and the peak-tone restoration again; the Loudness control can go above the audibility point with a warning.
- Status: Accepted. Implementation experimental.
- Implementation: `adjust` (`adjust.steps.v1`), `deesser::Plan::threshold_db` and `master adjust`. The preview, the manifest facts, the one-version finalizer, the WASM exports, the cloud action and the app panel are in D172.
- Decision or proposal: A pro account changes a delivered version with eight controls: Bass, Presence, Air, Sibilance, Loudness, Width, Compression and Ceiling. A step is 0.5 dB or 0.5 LU of measured change, 1 dB of compressor threshold, or 0.5 dB of ceiling. A control has at most 4 steps each way, and the ceiling goes down by at most 3 steps. The engine predicts each tone measure from the source spectrum and solves the filter gain of the step. It renders the full steps, then three quarters and half of them, as one version with `essential` as the reference, and solves the low-end hold and the peak-tone restoration again. The ceiling, the delivery verification, the mono fold-down, correlation and negative-band gates, and the low-end hold are hard gates. Loudness above the audibility point and a mean limiter reduction more than 0.3 dB above the parent are warnings. The size of a tonal change is reported. The compressor, the limiter and the de-esser keep their engine design, with no style selector.
- Reason: In the comparisons with the commercial limiter, compressor and de-esser (D168, D169, D170), the listener preferred the engine stage, and the differences were small or not audible. A style selector would offer differences that the listener does not hear. The 1.2.1 arms failed the criteria of an automatic version; a listener can select the same changes.
- Alternatives and tradeoffs: Profiles that change the compressor or the limiter. A blend of two delivered versions. Direct access to the spec. A final render that keeps the hold and the restoring gains of the preview, which gives a preview equal to the final file. Controls for the limiter attack and release and for the clipper budget: D167 and D168 measured no audible gain from them.
- Evidence: the advanced controls research, the advanced controls plan.
- Rule and code references: `adjust::candidates`, `adjust::worker_plan`, `adjust::render`, `adjust::widened`, `deesser::Plan::cut_db`, `selection::Gates::workload_as_warning`.
- Limits and counterexamples: The ranges have no listening result. The preview and the final render can differ, because the final render solves the hold and the restoration again. On study track 1, a request of +1 dB of bass at +1 LU delivered +0.74 dB.
- Verification: `adjust::tests` and `deesser::tests`. Two requests on study track 1 delivered verified files.
- Review condition: The listening check of the ranges, and the agreement of the preview and the final render.
- Work reference: TODO.md, "Advanced controls".
- Supersedes / superseded by: Superseded in direction by D199. The controls stay implemented until their removal.

### Updates

- 2026-10-06: User requirement: the pro tier changes each setting that it makes sense to change. Width, Compression and Ceiling join the controls. The agent instructions keep width automatic in the versions of a set only. The workload gate of an adjusted version becomes a warning (`Gates::workload_as_warning`). On study track 1 Full, Width +3, Compression +2, Ceiling −1 and Loudness +4 delivered +1.69 dB of side/mid, a compressor mean reduction of 2.28 dB against 1.49 dB, −1.00 dBTP and the loudness warning.
- 2026-10-07: A4 to A10 are implemented (D172). The cloud, the browser and the native render build a request from stored planner facts.
- 2026-10-09: The Bass measure is 40–250 Hz against 500 Hz–2 kHz, the reference of Presence. Before, it was 40–250 Hz against the K-weighted power of the whole band. On the Full versions of the 8 studio mixes, 40–250 Hz holds 58 to 81% of the K-weighted power, so a shelf raised the reference with the bass: +1 dB took a shelf of +3.1 to +7.6 dB, and +2 dB was out of reach on 5 mixes with a shelf of +12 dB. Against 500 Hz–2 kHz, ±2 dB takes a shelf of 2.3 to 2.8 dB. Bass +4 now delivers on 8 of 8 mixes, 7 at the full step, at +1.88 dB at the median, against 6 of 8, none at the full step, at +1.02 dB.

## D172: Paths of a request on a delivered version

- Recorded: 2026-10-07.
- Origin: User requirement: complete A4 to A10 of the advanced controls plan, the measurements and the advanced controls of the app, with a later gate to paid accounts.
- Status: Accepted. Implementation experimental.
- Implementation: `adjust::Facts`, `adjust::Base`, `adjust::finalize`, `adjust::render_in`, `adjust::preview`, `excerpt`, `distributed::Store`, the WASM exports `adjust_version` and `Preview`, the Lambda action `adjust`, and the bitneedle-app ADJUST panel.
- Decision or proposal:
  - The worker plan (`mastering.worker-plan.5`) and the manifest (`mastering.distributed-set.3`) store the planner facts of a request. The source spectrum is stored in 1/24-octave bands, so each path predicts from the same bands. Each manifest version stores its signature and the measures of the controls.
  - One function renders and finishes a request on each path: native, cloud and browser (`adjust::render_in` on a `distributed::Store`).
  - The duplicate screen of a request adds the measures of the controls, the compressor reduction, the de-esser cut and the ceiling to the signature of the set. A loudness or ceiling change makes a version distinct.
  - The name states the requested control. The Air measure moves with a Presence step, so a measured change of a control that is not requested does not name the version.
  - The span render keeps 4.5 s of context before the kept frames and 2.5 s after them, on a 15,360-frame grid.
  - The preview predicts the hold and the drive of the final render (`adjust::preview`).
  - The control Lambda names a request `<parent>-a<n>`, refuses a request over `ADJUST_LIMIT`, and with `ADJUST_ACCESS=pro` refuses a request without the `adjust` entitlement in the authorizer context. The app reads the result of the access check from the status and shows the pressing sheet.
  - The browser engine renders the segments of peak control in order. It has no threads.
- Reason: The cloud keeps a job for one day, so a request after that renders in the browser from the same stored facts. A spectrum of 4,097 bins in each manifest is larger than needed: the regions of the controls span an octave or more. The signature of the set leaves out bands that are not material and has no compressor measure: on track-g, Air −4 measured −1.96 dB and Compression −4 measured −1.15 dB, and the signature screen called both duplicates. On the test signal, 2.5 s of context left −98 dBFS of difference from the compressor state, and 4.4 s left none. The preview kept the parent's hold and drive, and on track-g it missed the final render by a median of 0.28 to 0.31 dB.
- Alternatives and tradeoffs: The full spectrum in the manifest. A request that the browser builds from its own analysis of the original: each path could then build a different candidate. A preview that renders the whole track for each change. The plain signature screen.
- Evidence: the advanced controls run.
- Rule and code references: `docs/RULES.md`, "Requests on a delivered version".
- Limits and counterexamples: The preview predicts the hold and the drive. It does not render them. A plan or manifest of engine 1.3.0 has no facts; the app deletes the sets of another engine version.
- Verification: `adjust::tests`, `excerpt::tests` and the control tests. The WASM `adjust_version` delivered a verified version of track-g in Node. The run is in the advanced controls run record.
- Review condition: The listening check of the ranges; the agreement of the preview and the final render on the 8 studio mixes.
- Work reference: TODO.md, "Advanced controls".
- Supersedes / superseded by: None.

### Updates

- 2026-10-07: Run on the 8 studio mixes. Before the run, a Bass step on Full delivered 63 to 66% of its change, because the builder predicted the shelf without the hold bells of Full. The builder now predicts each tone step on the tone with the hold and the low-mid correction of the parent, and Bass −2 delivers −0.99 dB at the median. The finalizer reads the delivered share from the chosen candidate. The preview keeps the parent's hold within the 0.5 dB tolerance of the hold. 86 of 102 preview checks pass. A span is within two least significant bits of 24 bits of the delivered render. The cloud status links to each result, because a result is about 0.5 MB.
- 2026-10-07: D173 renders each set at the rate family of its source. The worker plan (`mastering.worker-plan.6`) and the manifest (`mastering.distributed-set.4`) record the render rate, and the stored spectrum of the facts records its rate. The span render keeps 4.5 s of context before the kept frames and 2.5 s after them at the render rate. A span starts on one compressor section block, 10 ms at the render rate (`treatment_render::span_grid`), and the de-esser places its masking frames at their positions in the recording (`deesser::Plan::apply_from`). At 44.1 kHz the 15,360-frame grid was 451,584 frames, because 441 and 1,024 have no common factor. At 44.1 and 48 kHz, a span with no steps equals the same frames of the whole render under −138 dBFS, from a start on or off the masking frames (`excerpt::tests`).
- 2026-10-08: Engine 1.7.0. A Sibilance step changes the mean de-esser cut over the keyed frames by 0.5 dB: the threshold rises and the largest cut falls by a shift that the builder solves from the excess quantiles of the parent (`deesser::Stats::excess_quantiles_db`); before, a step of 2 moved the cut by less than 0.5 dB on each studio mix, and the duplicate screen held it back. `adjust::range` gives the range of each control on a version, and a request outside it is a result with the reason `outside-range`. Each version stores `hold_sections`, with its changes without a hold; the preview keeps the parent's hold wets plus the difference that the request makes to the first step of the hold, and takes its loudness from the mean wets of the enveloped filters (`Spec::mean_tone`). On the 8 studio mixes, 107 of 117 preview checks pass.
- 2026-10-09: The preview takes the two hold steps of the final render. Between the steps, it gives the rise of the first step from the stored spectrum after the tone of the version (`low_end::rise`). After a bass shelf, the bass is a larger share of the loudness, and the loudness match takes more of each bell's lift: on track-h Bass +2 the first step lifted the held bands by 0.9 dB against 1.55 dB from the source spectrum. Peak control changed them by −0.06 dB inside each window. track-h Bass +2 goes from 0.78 to 0.13 dB of median band difference; 108 of 117 preview checks pass.

## D173: Render at the rate family of the source

- Recorded: 2026-10-07.
- Origin: User requirement: master at the sample rate of the source, as long as the accuracy and the speed of the engine stay.
- Status: Accepted. Implemented as `set.render-rate.v1` (`audio_io::render_rate`, `dsp::Oversampler::new`, `dsp::OVERSAMPLER_CUTOFF_HZ`, `mastering.worker-plan.6`, `mastering.distributed-set.4`).
- Decision: The engine renders at 44.1 kHz for a source at 44.1, 88.2 or 176.4 kHz, and at 48 kHz for a source at 48, 96 or 192 kHz. Each filter, time constant, window and frequency band of the chain is set in hertz or seconds at the render rate. FFT lengths, filter lengths and the restoration window stay in samples. The interpolator and the decimator of peak control have a cutoff of 21.75 kHz at each rate. Stems are converted to the render rate of the mix. Each delivered file is at the render rate. A worker plan records the render rate, and a worker refuses a source at another rate.
- Reason: A 44.1 kHz source had a conversion to 48 kHz before the chain, and a second conversion for a 44.1 kHz delivery (D154). The conversion to 48 kHz also emptied the bins from 20.7 to 24 kHz, which made the masking model read each frame as tonal and lowered the audibility point.
- Alternatives and tradeoffs: Keep the 48 kHz chain and offer a 44.1 kHz copy: two conversions and a trim of up to 0.13 dB (D154). Render each source at its own rate, 88.2 and 96 kHz included: the evidence of each rule is at 44.1 and 48 kHz, and a rate above 48 kHz adds no content under 24 kHz. A cutoff of 0.90625 of the Nyquist frequency at each rate: at 44.1 kHz it took 12.6 dB from 20 kHz. Limit the spectral flatness of the masking model to 20 kHz: it changes the output at 48 kHz, and the item is open in TODO.md.
- Evidence: the source-rate run. At 48 kHz, the 23 versions of the eight confirmation mixes are identical to those of the earlier engine, byte for byte. On 44.1 kHz copies of the eight mixes, the native renders have the same versions and names as the masters of the 48 kHz originals. Their loudness is −0.10 to +0.30 LU from those masters, their largest third-octave change at matched loudness 0.11 dB, and their matched-gain null −38.6 to −78.2 dB. Through the 48 kHz path, the same copies were −0.10 to +1.10 LU from the native renders. Peak control at 44.1 kHz is flat to 20.5 kHz. The 16x true peak of a native version is −0.02 to +0.08 dB from that of the master of its original. At 48 kHz, the processor time of a set is unchanged (−2.0% to +0.4%). At 44.1 kHz it is 6.0% to 9.1% lower where the two paths make the same renders.
- Rule and code references: `set.render-rate.v1`; `audio_io::{render_rate, RENDER_RATES, Source::at_render_rate}`; `dsp::{k_weighting, Oversampler::new, OVERSAMPLER_CUTOFF_HZ, oversampled_per_ms}`; `set::WorkerPlan::sample_rate`; `distributed::render_family`; `tests/source_rate.rs`.
- Limits and counterexamples: Seven of the nine 44.1 kHz sources are conversions of 48 kHz mixes. The comparisons have no listening result. The audibility point of a 44.1 kHz copy is 0.24 to 0.40 LU above that of its 48 kHz original: the spectral flatness counts the bins above 20 kHz, which are a larger share of the bins at 48 kHz. The restoration window, the gain fit of the audibility point and the masking frames cover 8.8% more time at 44.1 kHz. The app reads delivered files at 48 kHz only (TODO.md, "Source-rate rendering").
- Verification: `cargo test --release` passes, with `tests/source_rate.rs`. The runs are in a local output directory.
- Review condition: A listening comparison of a 44.1 kHz mix rendered natively and through the 48 kHz path. A change of the spectral flatness of the masking model.
- Work reference: TODO.md, "Source-rate rendering".
- Supersedes / superseded by: Changes the render rate of D154.

### Updates

- 2026-10-07: The app plays, measures and delivers each set at its render rate (app f57be9b). D174 limits the spectral flatness of the masking model to 20 kHz.
- 2026-10-16: Keep decision for the listening check Render rate. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16. regression mix 1 and regression mix 2, converted to 48 kHz with FFmpeg (Kaiser-windowed sinc, 256 taps) and rendered through the 48 kHz path, against their native 44.1 kHz renders: loudness −0.06 to +0.09 LU, true peak within 0.01 dB, 50 ms crest within 0.01 dB, the same versions and names. At matched loudness, less the difference that the conversion alone gives the mix in the bands of `master inspect`, each band is within 0.07 dB, and within 0.26 dB in the 16 kHz band, which regression mix 1 leaves empty. Before D174, the 48 kHz path was up to 1.10 LU from the native render. The native rate stays: it has two conversions fewer and takes 6.0% to 9.1% less processor time. The measurements are in the acoustic decisions run.

## D174: Spectral flatness of the masking model up to 20 kHz

- Recorded: 2026-10-07.
- Origin: User requirement: the masking model takes no account of the bins above 20 kHz.
- Status: Accepted. Implemented as `masking.flatness-range.v1` (`masking::FLATNESS_MAX_HZ`), engine 1.6.0.
- Decision: The spectral flatness of each masking frame, which sets the tonality offset, uses the bins from the first bin above 0 Hz to 20 kHz. The band powers, the thresholds and the rest of the chain keep each bin up to the Nyquist frequency.
- Reason: The flatness is the ratio of the geometric mean to the arithmetic mean of the bin powers. Low-level bins lower the geometric mean. Above 20 kHz the level depends on the sample rate, on the filters and on the codecs that the mix went through, and the content is inaudible. Before D174 these bins made each frame more tonal, which lowered the masking threshold and the audibility point. The bins above 20 kHz are 17% of the bins at 48 kHz and 9% at 44.1 kHz, so the point also depended on the rate.
- Alternatives and tradeoffs: Count the bins above 20 kHz when they hold content: the content above 20 kHz of the 48 kHz originals is 50 to 72 dB under the band from 1 to 2 kHz and still lowers the geometric mean, and a content threshold makes the point change in a step. A tonality per critical band (Johnston): a larger change; the bins from 15 to 20 kHz also lower the flatness of one value per frame.
- Evidence: the tonality range run. On the eight confirmation mixes at 48 kHz, the audibility point rose by 0.56 to 0.93 LU. The base target changed on the two mixes where the point capped it: track-c by +0.25 LU and track-f by +0.24 LU. The difference between the points of a 44.1 kHz copy and of its 48 kHz original went from +0.24 to +0.40 LU to −0.03 to +0.06 LU. Each set kept its versions and names. Where the target did not change, the delivered files are identical or have a null of −61 dB or lower.
- Rule and code references: `masking.flatness-range.v1`; `masking::{frames, FLATNESS_MAX_HZ}`; `masking::tests::flatness_ignores_content_above_20_khz`. The flatness also sets the masking of `stem_evidence` and `low_mid`.
- Limits and counterexamples: On regression mix 1, the target rose by 0.28 LU, and the limiter and the clipper did more work. At matched loudness, the 16 kHz third octave of its versions rose by 0.37 to 0.80 dB. The comparisons have no listening result.
- Verification: `cargo test --release` passes. The runs are in a local output directory.
- Review condition: A listening result on a mix where the target rose. A tonality per critical band.
- Work reference: TODO.md, "Source-rate rendering".
- Supersedes / superseded by: Changes the tonality input of the masking model of D173 and of the audibility point.

### Updates

- 2026-10-16: Keep decision for the listening check Spectral flatness. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16. With the flatness over every bin, as before this decision, regression mix 1 renders 0.28 LU lower and track-f 0.69 LU lower, at the base target of D179. At matched loudness, each band of track-f is within 0.06 dB. On regression mix 1, the 16 kHz band of `master inspect` is 1.19 to 1.59 dB lower, and every other band is within 0.03 dB. Regression mix 1 has no content there: the band is 84 dB under the 1 kHz band in the mix, and 62 to 66 dB under it in the versions, where peak control fills it. Content above 20 kHz is outside the range of hearing, and the margin of D179 comes from the audibility point of this decision. The flatness range stays. The measurements are in the acoustic decisions run.

## D175: Anti-phase instrument group as a source condition

- Recorded: 2026-10-12.
- Origin: track-c. The user heard the Full version as the mix with its stereo removed, judged the mix fine, and asked that the engine detect when a narrowing is needed and otherwise leave the width. The phase study found one instrument group polarity-inverted while the mix's whole-file correlation stayed +0.22, so D142 did not fire.
- Status: Provisional. Implemented in `stem_evidence::stem_image`, `stem_evidence::Evidence::stem_images`, `source_condition::antiphase_stems` and `source_condition::AntiphaseStem` as `set.source-condition.v2`.
- Decision: With aligned stems, the stem analysis measures the stereo image of each stem (`stereo::analyze`): per band, the whole-file correlation, cross-spectrum phase and resultant, the seconds of active and of negative 0.5 s windows, and the first and last negative window. The source condition reports a stem in a band when 50% or more of its active windows are negative (correlation under −0.25), its phase is beyond 135° with a resultant of 0.3 or more, and every other stem active in the band (10% or more of the longest active time) is within 45° of in phase. The report carries the role, the bands, the time range and the share. The engine changes no processing for it: the width of the mix stands (D132, 2026-10-12), and the alignment of D143 answers a whole-mix anti-phase band.
- Reason: A mastering stage cannot repair one instrument inside a stereo mix. The condition tells the user what a mono listener loses and where, and the mix engineer can flip the polarity in a minute. The whole-file correlation of the mix hides the condition behind the in-phase vocal, and the stem's own whole-file correlation stays above −0.5 because its in-phase intro dilutes it, so the share of negative windows carries the test.
- Alternatives and tradeoffs: Narrowing the mix (withdrawn, D132). Aligning the mix's phase (D143): the phase of the mix is incoherent where the vocal and the inverted group meet (resultant 0.05 in the region), so no single rotation applies. Thresholds on the stem's whole-file correlation: −0.18 on track-c, under the test.
- Evidence: track-c: the `other` stem from 0:25 to 2:52 in the two midrange bands, 62% negative windows, −166°. track-b and the earlier mix of track-d: none. Drums, bass and vocals of track-c within 14° of in phase. The measurements are in a local output directory.
- Rule and code references: `set.source-condition.v2`; `source_condition::STEM_ANTIPHASE_SHARE`, `STEM_ANTIPHASE_DEGREES`, `STEM_ANTIPHASE_RESULTANT`, `STEM_INPHASE_DEGREES`, `STEM_ACTIVE_SHARE`.
- Limits and counterexamples: The stems are estimates and the `other` stem groups guitars, keys and synths, so the report names the group, not the instrument. The condition does not say whether the inversion is a choice or a fault. The MASTER screen does not show it yet. A stem analysis adds one stereo analysis per stem to the planner.
- Verification: `source_condition::antiphase_stem_tests` (one inverted stem among in-phase stems; a whole mix out of phase; a mild share and a quiet stem). The suite, clippy and fmt pass. No listening result.
- Review condition: The sixteen sources; the copy of the MASTER screen; a mix whose inverted instrument is in the vocal or drum stem.
- Work reference: TODO.md, "Version tone" and "Advanced presets".
- Supersedes / superseded by: Extends D142.

### Updates

- 2026-10-12: The sixteen sources (the files of this run are no longer kept): track-c reports the `other` stem in the two midrange bands from 0:25 to 2:52 (62% negative windows, −166°); the fourteen other studio and stress sources report no stem. stress recording 3, a live recording whose mix is anti-phase from 6 to 20 kHz (D142), reported its drum stem alone from 12 to 20 kHz, where no other stem is active. Two conditions were added: a band that D142 reports on the mix is left out (the alignment of D143 answers it), and another stem must be active in the band. stress recording 3 then reports no stem, and its Mono-safe preset (a −6 dB bell at 15.5 kHz) is gone.
- 2026-10-16: The web MASTER screen shows each reported group with its time range (bitneedle-app `46b1764`, `antiphaseNotes` in `web/js/mastering-source.mjs`): under the title on `/master` and on the Original card, from `source_condition.antiphase_stems` of a cloud manifest or of the summary of a local set. The copy for track-c reads: "From 0:25 to 2:52, the left and right channels of the other instruments, such as guitars, keys and synths, are out of phase, so they cancel out on a single speaker. The masters keep this as it is. If you did not intend it, flip the polarity of one channel of that part in your mix." The other roles are "the drums", "the bass" ("it cancels") and "the vocals". Two or more groups end with "The masters keep these as they are. If you did not intend them, flip the polarity of one channel of each part in your mix." The times are the start of the first and the end of the last negative window, rounded down to the second. The native app does not show it. The agreement of the user for the copy is open.

## D176: Mono-safe preset

- Recorded: 2026-10-12.
- Origin: User decisions of 2026-10-12: the ADJUST panel is driven by presets that the engine derives from the measured source; a preset that answers a condition appears only when the engine detects it; the width of a mix stands by default.
- Status: Provisional. Implemented in `width::mono_safe`, `adjust::Facts::mono_safe`, `adjust::Preset`, `adjust::Request::preset`, `adjust::preset_candidates`, `adjust::preview_preset`, `adjust::range` ("presets"), `adjust::finalize`, `master adjust --preset`, the cloud action `adjust` (`preset` in place of `steps`) and the WASM `Preview` (`{"preset": NAME}` in place of the steps), as `adjust.mono-safe.v1`. The ADJUST panel of bitneedle-app offers it from `range.presets` with the copy "Mono-safe — Holds the parts that cancel on a single speaker. Everything else stays as wide as your mix." (agreed with the user on 2026-10-12); the WASM build, the cloud and the site are not deployed.
- Decision: When the source condition reports an anti-phase instrument group (D175), the planner stores, in the facts of the set, one side bell per reported band: centred on the band with the band as its bandwidth, at the gain that brings the band's P10 window correlation to +0.05, at most −6 dB, with an envelope that is 1 in each 0.5 s window whose correlation is negative and in the window on each side, and 0 elsewhere, joined by the raised cosine of the focus envelopes. A request with the preset renders the parent's spec with the bells on its tone at the full gain, three quarters and half, under the gates of a request, with the hold and the restoration solved again. The result is named "Mono-safe" after its parent, and its description gives the bands, the share of the song that the cut covers and the mono fold-down loss of those bands in the mix and in the result.
- Reason: The default versions keep the width of the mix (D132, 2026-10-12). A listener who needs the mix to hold up in mono can ask for it, and the engine sets the amount and the passages from the measurements, so the user stays out of the engineering. The keyed cut leaves the in-phase passages and bands as they are.
- Alternatives and tradeoffs: A static narrowing (withdrawn, D132). A width dial (D171): the user sets an amount with no measurement behind it. A per-band static cut: the intro and the vocal passages lose width for nothing.
- Evidence: track-c: two bells, 1 kHz at −4.1 dB and 3.46 kHz at −4.8 dB, in 52% of the windows; delivered at the full amount; the description reads "narrower from 500 Hz to 6 kHz where the channels cancel, 52% of the song. Mono loss 2.2 → 0.8 dB." Whole-file side/mid of the midrange against the parent: −3.5 → −6.3 dB and −4.2 → −7.6 dB; P10 window correlation −0.20 → +0.23 and −0.18 → +0.34. The low mids move 0.4 dB and the 6 to 12 kHz band 1.1 dB from the skirt of the 3.46 kHz bell.
- Rule and code references: `adjust.mono-safe.v1`; `width::MONO_SAFE_RULE`, `width::mono_safe`.
- Limits and counterexamples: One source. The bell's skirt reaches the band above by about 1 dB. The cut turns the inverted instrument down where it applies, as a side cut does (D132, 2026-10-12). No listening result. The range of the preset is one amount; the shares of a request stand in for a range.
- Verification: `width::tests::mono_safe_cuts_the_side_only_where_the_channels_cancel`, `adjust::tests::the_range_gives_the_first_share_of_each_step` (the preset is absent without the condition). The suite, clippy and fmt pass; the Lambda crate checks.
- Review condition: A listening check on track-c of 0:45 to 1:10 and 1:33 to 1:55 against the parent; the sixteen sources; the panel copy.
- Work reference: TODO.md, "Advanced presets"; the advanced presets plan, P1 to P5.
- Supersedes / superseded by: Extends D171 toward its successor.

### Updates

- 2026-10-12: Listening result. The user heard both passages of the listening clips on AirPods, Full against Mono-safe in stereo and summed to mono: "they all sound good". The stereo half of the check stands as heard; the mono half is the summed clips, which are what a single speaker plays. The user then asked what the choice is, since a stereo master that holds up in mono is what everyone wants and a pure mono mix has no use case. Decision: the keyed cut is the default of the treated versions (`set.mono-safe.v1`). When D175 reports an instrument group, the planner puts the bells of `width::mono_safe` in the spec of every candidate of Full and Forward (`Spec::mono_safe`, merged into the effective tone with the hold and the low-mid correction); Essential keeps the image of the mix. The description of a treated version then reads "Holds the parts that cancel on a single speaker". The Mono-safe preset stays for Essential alone: `adjust::range` offers it when the parent carries no bells, and a request on a version that carries them is refused. The reasons: the condition is rare (one of sixteen sources), the cut acts only where the channels cancel, the listening found no harm in stereo, and most listening is on a phone or one speaker.
- 2026-10-12: The cut renders as the zero-phase side filter of the widening (`width::SideGain`, third-octave band edges) with the band's gain, blended by the envelope after the side gain (`treatment_render::apply_mono_safe`), instead of a biquad bell in the tone. On track-c Full against the version without the cut: 6 to 12 kHz −6.4 against −6.1 dB (the bell gave −7.2), 120 to 500 Hz −6.6 against −6.7 dB (the bell gave −7.1), and the two midrange bands −6.4 and −7.5 dB as with the bell. The bell of `Spec::mono_safe` now gives the band and the gain only. `treatment_render::tests::a_mono_safe_cut_stays_in_its_band_and_its_passages` checks the dry passage is untouched and the bands beside the cut move by under 0.6 dB.
- 2026-10-12: Phone-speaker listening result, the same two passages of Full against Mono-safe played on a phone's own speaker: the cut's movement "audible and distracting"; the guitars and keys come back "somewhat"; preference Full. On AirPods the same clips had sounded good in stereo and summed to mono. The default of the same day is withdrawn: no version carries the keyed cut as made, and `set.mono-safe.v1` leaves `docs/RULES.md`. The user then judged the treatment itself bad ("mono safe is bad"), so the preset is withdrawn too: `adjust::MONO_SAFE_OFFERED` is false, no set offers it and a request for it is refused. The detection of D175 stays. The machinery stays for a form whose movement a phone speaker does not show, such as a longer glide, no neighbour windows, a cut only in runs of 2 s or more, or a smaller cut, each with a phone-speaker check (TODO.md).
- 2026-10-17: Keep decision for the Mono-safe form (TODO.md). Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16. Forms: a glide of 2 s, no neighbour windows, a cut only in negative runs of 2 s or more, and a cut of at most 3 dB, each one change from the form of 2026-10-12, and the four changes together. Each form rendered on track-c as the preset of a request on `master` of engine 1.9.0, and each is delivered at its full share. Criterion, in place of the phone-speaker check: in each band of one ERB from 500 Hz to 10 kHz, at each ear, the level change of the passages with the cut against those without it, and the 90th percentile of the steps at the changes of the cut, are 0.21 dB or less. 0.21 dB is half the smallest increment that a listener detects in wideband noise whose level is high for 1.5 s and low for 4.5 s: ΔI/I = 0.1, or 0.41 dB (Miller 1947, J. Acoust. Soc. Am. 19, 609–619; Allen and Neely 1997, J. Acoust. Soc. Am. 102, 3628–3646, Fig. 5). Models: one speaker that plays the mono sum, and a phone in landscape with two speakers 14 cm apart at 32 cm, the mean distance at which people read a web page on a phone (Bababekova et al. 2011), with the spherical head of Brown and Duda (1998). Both pass the band of a phone speaker, 500 Hz to 10 kHz. Measurements: with one speaker, no form changes a band by more than 0.04 dB. A side cut leaves the mono sum as it is, so the parts that cancel stay cancelled. With two speakers, the side reaches the ear 7.1 to 11.2 dB above the mid from 3 to 5 kHz, against 0 dB on headphones. Each form moves the bands from 2 to 6 kHz by 2.56 to 4.33 dB there, with steps of 2.64 to 4.43 dB at P90: 6 to 11 times the detected increment. At 20 and 60 cm the depth is 1.96 to 4.13 dB. The forms change the number and the speed of the changes, not their depth. The clips of 2026-10-12 measure 3.06 and 3.67 dB with two speakers and 0.13 dB or less with one speaker: the two-speaker model gives the movement that the user heard. A cut that meets the criterion with two speakers is about 0.24 dB, under the smallest move of a stage (0.5 dB). No form passes, and `adjust::MONO_SAFE_OFFERED` stays false. The album master of track-c widens 2 to 6 kHz by 2.7 dB and keeps a mono fold-down loss of 3.6 dB there (D177). The keyed narrowing of the Radio and mono playback presets (TODO.md) has the same two properties: no change of the mono sum, and a movement on a phone with two speakers.

## D177: The album pairs of one engineer as the prior

- Recorded: 2026-10-13.
- Origin: Source observation. Eight album masters of the album by the reference engineer arrived on 2026-10-13.
- Status: Provisional.
- Implementation: Absent. The pairs are measured (the reference album run); no rule reads them.
- Decision or proposal: The eight pairs replace the single track-e master as the reference of the set rules. Where a rule's bounds come from the corpus mixes the pair prior is the comparison, dimension by dimension; a rule changes only with a decision that names the dimension, the pair evidence and the listening result. The dimensions where the engine and the pairs differ by more than the pairs' interquartile range are recorded here as open work, not as changes.
- Reason: One master was an example at a brief the album did not keep. Eight masters of one brief give a distribution of outcomes and of moves conditional on the mix. The album is also the material the engine is compared against in listening.
- Alternatives and tradeoffs: Treat the pairs as targets (rejected under D112: the moves differ in sign between tracks in the low mids and the presence, so a target would be the median of opposite decisions). Keep the single master as the reference (rejected: the engineer did not keep its brief).
- Evidence: the reference album run. Medians of the moves over the eight: sub − bass +1.38 dB, bass − body +1.17 dB, low mid − body +0.40 dB, presence − body +0.13 dB, slope −0.17 dB per octave, side/mid +2.10 dB, loudness +3.49 LU, crest −4.05 dB, true peak −0.53 dB, 50 ms crest P50 −0.38 dB. Widening per band, medians: +4.0 dB under 120 Hz, +4.7 dB from 120 to 500 Hz, +2.0, +1.5, +1.5 and +1.3 dB above. P90 attack loss 2.7 to 4.9 dB. Sibilant frames move with the other vocal frames within 1.1 dB. The engine's `master` version on the same mixes: presence − body +1.74 dB, sub − bass −0.31 dB, bass − body −0.37 dB, slope +0.44 dB per octave, side/mid +2.19 dB, loudness +3.94 LU, widening +0.6 and +2.0 dB in the two low bands and +1.8 dB above; louder than the pairs by 0.8 to 1.4 LU on four tracks.
- Rule and code references: the pair measurement script; `set.low-end.v1`, `set.widening.v3`, `set.audible-loudness.v2`, the presence rules of D164.
- Limits and counterexamples: One engineer, one artist, one mixer, eight tracks. The engineer's own two masters of track-e differ by 2 LU and 3.6 dB of presence, so the prior is of a brief, not of the engineer. The low-mid and presence moves change sign across tracks. The widening under 120 Hz leaves the band 14 to 33 dB under the mid. The track-h master ends with a P10 window correlation under −0.5 in four bands; its cause is not measured.
- Verification: Measurements only; no listening of the pairs against the engine yet.
- Review condition: A listening comparison of the engine's versions against the album masters on the eight tracks, with the user's preference per dimension; the loudness decision of the user (the engine is louder on four tracks).
- Work reference: TODO.md, "Album pairs".
- Supersedes / superseded by: Extends D112; the track-e reference of D031 is one pair of nine.

### Updates

- 2026-10-13: Open dimensions, from the medians above: the engine's presence lift against the body is 1.6 dB (master) to 3.0 dB (bold) above the pairs; its low end is 1.5 to 1.7 dB under the pairs in sub − bass and bass − body; its widening is 2.7 dB under the pairs from 120 to 500 Hz and 3.4 dB under them below 120 Hz, where `set.low-end.v1` holds the mix; it is 0.8 to 1.4 LU louder on four tracks; it keeps 1.5 dB more attack. Each is open work, not a change.
- 2026-10-14: The pair measurement script measures the offset (`analysis::align_offset`), the attack and crest losses, the image per band, the sibilance at the offset and the engine's versions in one run. The offsets agree with the hand lags within 3 ms. Sibilance with the repository probe at the offsets: the sibilant P99 moves with the other vocal frames within 0.9 dB on the seven tracks with stems. The single master of track-e at its offset of 286 samples lowers the P99 by 0.97 dB and the other frames by 0.77 dB; the −4.5 dB of 2026-10-13 was measured 60 ms off. The engine's versions against the masters, medians: `open` side/mid +2.08 against +2.10, loudness +3.45 against +3.49 LU, sub − bass +0.79 against +1.38 dB; every version loses 0.8 to 1.3 dB less attack at P90 than the masters.
- 2026-10-16: The P10 of the track-h master under 0.05 has two causes: one channel of the master has the opposite polarity from 0:00 to 0:33, and the master widens a passage from 2:18 to 2:38 whose low end is already decorrelated. The low-mid width decision is D183. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- 2026-10-16: The listening check of the album pairs ends in the decisions D179 to D186 for loudness, presence, the de-esser, the low end, the low-mid width, the widening, the low-end hold and the width of `open`. Against the pairs, `master` now has 5 of 8 tracks inside the pair range in presence against body, 5 in low mids against body, 6 in bass against body and 2 in sub against bass, and each version is 0.04 to 0.94 LU under its album master. Sub against bass stays under the pairs on six tracks (TODO.md, "Sub lift"). Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D178: Open version from the album pairs

- Recorded: 2026-10-13.
- Origin: User requirement: "lets do a best effort based on analysis of all the reference masters", after the eight album masters of D177 were measured.
- Status: Provisional.
- Implementation: Implemented as `set.open.v1`: `open::plan`, `open::Open`, `width::OPEN_PATTERN_DB`, `width::OPEN_WINDOW_CORRELATION`, `width::side_allowance_within_db`, `width::Widening::open`, `set::OPEN_LABEL`, the `open` branch of `set::branches`, `version_names::Character::Open`, `version_names::designed`.
- Decision or proposal: Every set delivers a fourth version, `open`, in its own family. Its tone is the tone of the mix with two stereo low shelves: +1.4 dB at 90 Hz (the median sub-against-bass move of the pairs) and +1.2 dB at 300 Hz (the median bass-against-body move), each reduced where the mix already reaches the P75 outcome of the pairs (+3.44 dB sub against bass, +6.07 dB bass against body). Its width is the median widening of the pairs per band (`OPEN_PATTERN_DB`: +4.0, +4.7, +2.0, +1.5, +1.5 and +1.3 dB from the band under 120 Hz up), bounded by each band's allowance with the P10 window bound at −0.25 instead of 0, the correlation-drop and mono-loss bounds unchanged; the band under 120 Hz widens, which no other version does. Its target is 0.5 LU under the base target. It carries no de-esser, no low-mid correction, no presence move and no low-end hold. Fallbacks: three quarters and half of the tone, then half the widening. The stereo gates of the selection (mono fold-down, correlation drop, no band turned negative) stand. The version is designed Open for the naming and keeps the name when its bass and width steps both measure 0.5 dB or more.
- Reason: The album masters agree on one taste and differ from the engine in its shape: more low end, a wider bass and low midrange, a flat tone and a lower level (D177). No delivered version has that shape; Wide and Forward widen the midrange and the treble, Bold adds presence. A version that carries the shape lets the user hear it against the album masters on the eight tracks and decide which dimensions the defaults take.
- Alternatives and tradeoffs: A request preset (rejected for now: the shape is a whole taste, not a step on a version). Changing the defaults (rejected: no listening result). Amounts from the mix alone under D112 (not possible for a taste the rules do not derive; the pairs' medians are the amounts, the pairs' P75 outcomes the bounds, and the version says so in its basis).
- Evidence: the reference album run, "The Open version against the album masters". On the eight mixes the version delivers, from its first candidate, medians against the album pairs of: side/mid +2.08 against +2.10 dB, per band within 0.2 dB; loudness +3.45 against +3.49 LU; crest −4.04 against −4.05 dB; 50 ms crest −0.40 against −0.38 dB; slope −0.21 against −0.17 dB per octave; sub − bass +0.79 against +1.38 dB; bass − body +0.91 against +1.17 dB; presence − body −0.23 against +0.13 dB; low mid − body +0.85 against +0.40 dB.
- Rule and code references: `set.open.v1`; `set.widening.v3` for the allowance; `set.audible-loudness.v2` for the base target.
- Limits and counterexamples: The amounts are the medians of one engineer's album; D112 does not derive them. The shelf gains are solved for the predicted region moves on the spectrum of the mix, and the compressor and peak control then take about 0.6 dB of the sub against the bass: the delivered sub move is 0.79 dB of 1.4. The 300 Hz shelf lifts the low mids against the body by 0.45 dB more than the pairs. track-c takes no midrange widening under the window bound. The P10 window bound of −0.25 admits passages that cancel in mono; a phone-speaker check is open. The band under 120 Hz widens in a version for the first time; a mix whose low end already carries 2% or more of side power keeps the side high-pass. No listening result.
- Verification: `open::tests`, `width::tests::the_open_allowance_admits_negative_passages_to_its_own_bound`, `set::tests` (the branch), the suite; the renders of the eight mixes.
- Review condition: The listening comparison of TODO.md, "Album pairs": the Open version against the album master and the other versions on the eight tracks, on monitors and on a phone speaker.
- Work reference: TODO.md, "Album pairs".
- Supersedes / superseded by: Extends D177 and D132.

### Updates

- 2026-10-14: `set.open.v2`. User decision: the version stays a fourth version of the set and is delivered only where it fits the mix ("I want this to be a fourth if it fits the gates"); the alternative of folding it into `bold` was not taken, since `bold` adds presence and midrange width and Open adds low end and bass width with no presence move. Two bounds from the pairs: the Open widening of a band stops at the largest side/mid the eight masters reached in the band (`width::OPEN_OUTCOME_MAXIMUM_DB`: −13.8, −1.4, −0.1, +1.1, −2.6 and −4.0 dB from the band under 120 Hz up), and a set plans the version only where both bands under 500 Hz admit at least 1 dB of the widening after the bounds (`width::OPEN_FIT_MINIMUM_DB`), the condition of the eight mixes, which had a narrow, correlated low end and took 2.6 to 5.4 dB there. `width::Widening::open_withheld` states the reason when a set withholds it. On the source images: all eight album mixes fit, track-c with 1.9 dB from 120 to 500 Hz; two mono live recordings and a game soundtrack with a sub 3.4 dB wider than any album master are withheld; two other stereo mixes fit. Reason: a gate on resemblance to the eight mixes would refuse a mix that has the condition his move answers and admit one that lacks it; the condition is what the pairs evidence. `width::tests::the_open_version_fits_a_narrow_low_end_and_not_a_wide_one`, `set::tests` (the branch is absent when the widening is withheld).
- 2026-10-16: The Open version has the base target. The level step of 0.5 LU is removed: the base target of `set.audible-loudness.v3` (D179) sits at the level of the album masters. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- 2026-10-16: `set.open.v3` (D186). The Open widening keeps the P10 window bound of the other versions, 0; the bound was −0.25. track-c and regression mix 2 deliver no `open`. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D179: Loudness margin under the audibility point

- Recorded: 2026-10-16.
- Origin: The loudness decision of D177 (TODO.md, "User decisions"). The album masters deliver −9.2 to −11.5 LUFS, and the engine was 0.8 to 1.5 LU louder on four tracks.
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `set::AUDIBILITY_MARGIN_LU`, `set::density`, `set::prepare`, `audibility::render_at` and `audibility::Point::drive_for` as `set.audible-loudness.v3`.
- Decision: The base target is the crest target of `set.source-density.v4`, at most 2.75 LU under the audibility point of peak control (D131). The cap was 0.5 LU under the point before. The Open version has the base target; D178 set it 0.5 LU under the base target. The base render of the planner, which sets the widening and the limiter attack, searches its drive from the probes of the point and joins them.
- Reason: Streaming services normalize playback to about −14 LUFS (Spotify, YouTube, Tidal, Amazon Music) and −16 LUFS (Apple Music). A louder master plays at the same loudness after normalization, and each LU of extra level costs crest and attack in peak control. The eight album masters sit 1.94 to 2.69 LU under the audibility point of their mixes (median 2.36 LU). At a margin of 2.75 LU, no version is louder than the album master of its mix.
- Alternatives and tradeoffs: A margin of 2.5 LU, five steps of the 0.5 LU step of D131: track-b 0.2 LU and study track 1 0.05 LU louder than their album masters. A cap at the loudest album master, −9.2 LUFS: track-f stays 1.45 LU above its master. A target of −14 LUFS: no loss after normalization, but 3 to 5 LU under the album masters.
- Evidence: the acoustic decisions run. On the eight album tracks each version is 0.04 to 0.94 LU under its album master (median 0.38 LU). The base target falls by 0.4 to 1.8 LU on six tracks; the crest rule sets it on track-h and track-d. Medians of `master`: crest move −4.65 → −3.40 dB, P90 attack loss 3.72 → 2.72 dB, largest limiter reduction 1.81 → 1.40 dB. regression mix 2 falls from −10.99 to −13.24 LUFS and regression mix 1 from −11.81 to −14.06 LUFS. On the ten sources the versions, names, chosen candidates and limiter attacks do not change.
- Rule and code references: `set.audible-loudness.v3`; `set::AUDIBILITY_MARGIN_LU`; `audibility::render_at`, `audibility::RENDER_TOLERANCE_LU`.
- Limits and counterexamples: One engineer and one album set the margin. A player without loudness normalization plays the masters up to 2.3 LU quieter than before. D155 recorded a user requirement that a change does not make the masters quieter; this decision lowers eight of ten sources. The margin uses the point of the `essential` prefix; a version with more low end has a lower point (D164).
- Verification: `audibility::tests::the_drive_under_the_quietest_probe_falls_one_db_per_lu`; the density assertion of `tests/set.rs`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass. No listening result.
- Review condition: A listening comparison at delivered level without normalization; a second album of mix-and-master pairs.
- Work reference: TODO.md, "User decisions", Loudness (D177), removed.
- Supersedes / superseded by: Supersedes `set.audible-loudness.v2` (D131) and the level step of the Open version (D178).

### Updates

- 2026-10-16: The track levels of the album follow the level relations of the album masters. Over the seven adjacent pairs of tracks in album order, the level step of `master` differs from that of the album masters by 0.41 LU RMS, at most 0.77 LU (track-b to track-e), against 1.16 LU RMS at `ee64d96`. With the mean offset taken off, the level of each track differs by 0.31 LU RMS, against 0.71 LU. The measurements are in the acoustic decisions run. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

- 2026-10-18: `bold` targets 0.5 LU under the audibility point (D203, `set.bold-level.v1`). The other versions keep this rule.

## D180: Presence moves within the album pairs

- Recorded: 2026-10-16.
- Origin: TODO.md: the user decision Presence (D164, D177), the listening check Presence and treble steps of `bold` (D161) and the engine item Presence ceiling (D134).
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `broad_tone::presence_move_maximum_db`, `broad_tone::PRESENCE_MOVE_MAXIMUM_DB`, `broad_tone::VIVID_PRESENCE_MOVE_ABOVE_DB`, `broad_tone::PRESENCE_OUTCOME_MINIMUM_DB`, `broad_tone::analyze` and `broad_tone::vivid_tone` as `set.presence-balance.v2` and `set.vivid-steps.v2` in `set.broad-tone.v6`.
- Decision: The static tone of `master`, with the lead bells, the shelves and the widening, moves presence (2 to 5 kHz) against body (500 Hz to 2 kHz) by at most +0.94 dB, the P75 move of the album pairs, and never past the equal-loudness balance of D134 (−3.30 dB). A mix darker than the darkest album master (−9.10 dB) may move up to that outcome, by at most 2 dB. The lead presence bell takes no more than the bound leaves after the shelves and the widening; the presence balance bell adds the rest. `bold` steps its presence 1 dB above `master`, at least to the gain of `master`, and its whole tone with its own widening stays within the bound of `master` plus 0.48 dB, the P90 move of the pairs (+1.42 dB) less their P75 move. The floor of 1.5 times the presence gain of `master` (D161) is removed, and with it `set::BOLD_FACTOR`. The treble step of `bold` (0.5 dB, D161) stays. `essential` keeps the tone of the mix.
- Reason: The album masters move presence against body by +0.13 dB at the median and −0.66 to +0.94 dB from P25 to P75, in both directions, and their outcome (median −4.83 dB) stays under the equal-loudness balance. The engine moved it by +1.92 to +2.89 dB in `master` and +2.37 to +4.20 dB in `bold`. A lead bell lifted track-a, a mix 1.43 dB above the balance (D134). `bold` is the brighter version, so it takes the P90 side of the pairs.
- Alternatives and tradeoffs: The median move of the pairs for `master` (+0.13 dB): no presence move on most mixes. The bound on the presence balance bell alone: the lead bell kept up to 2 dB of presence lift. A bound on the static tone of `bold` without its widening: `bold` measured 1.22 to 2.20 dB.
- Evidence: the acoustic decisions run. Presence − body move, medians over the eight album tracks: `master` +2.44 → +1.90 dB, `bold` +3.58 → +1.43 dB. The presence region of `master` falls by 0.4 to 1.4 dB against the mix at matched loudness; the body cut of the bass step (D164) lowers its body by 0.9 to 3.5 dB and gives the rest of its move. track-a: `master` +2.15 → −0.37 dB, with no presence move. Air − presence of `master` −0.28 → +0.37 dB; slope of `bold` +0.53 → +0.38 dB per octave. All versions are delivered on the ten sources.
- Rule and code references: `set.presence-balance.v2`, `set.vivid-steps.v2`; `broad_tone::presence_move_maximum_db`, `broad_tone::reference_presence_db`, `broad_tone::source_presence_db`.
- Limits and counterexamples: The body cut of the low-end hold and of the bass step raises presence against body after the static bound, in `master` and in `bold` (TODO.md, "Low-end hold (D159)"). Where the widening of `bold` carries its presence past the bound, its presence balance bell is at 0 dB (track-f). Without the presence balance bell, the focus bells of track-c and regression mix 2 fail the gate of the lead bell, and `master` takes candidate 3, without the focus bells. One album sets the bounds.
- Verification: `set::foundation_plan_tests::vivid_steps_presence_and_treble_and_keeps_corrections` checks the bound of `bold`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass. No listening result.
- Review condition: A listening comparison on monitors and on a phone speaker; a second album of pairs.
- Work reference: TODO.md, "User decisions", "Listening checks" and "Engine", removed.
- Supersedes / superseded by: Supersedes `set.presence-balance.v1` (D134) and `set.vivid-steps.v1` (D161).

### Updates

- 2026-10-17: A captured recording takes `set.capture-presence.v1` (D200): its `master` moves up to the darkest album master without the bound of 2 dB, with a presence balance bell of up to 6 dB. Basis: acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.

## D181: No keyed de-essing in the versions of a set

- Recorded: 2026-10-16.
- Origin: TODO.md: the user decision De-esser (D133, D170, D177), and the listening checks De-esser depth (D153) and Air shelf and de-esser (D162).
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `deesser::VERSION_CUT_DB` and `deesser::Plan::from_key` as `set.deesser.v3`. Since 2026-10-23, the rule `set.deesser.v4` of [D209](#d209-keyed-de-essing-from-the-90-excess-of-the-recording) replaces `set.deesser.v3`, and the default of this decision stays in force through [D209](#d209-keyed-de-essing-from-the-90-excess-of-the-recording).
- Decision: The de-esser plan of `master` and `bold` keeps the key of D133 and D153 and its measure of each keyed frame, with a largest cut of 0 dB: no frame is cut. The Sibilance control of a request (D171) cuts from the plan: a step down lowers the threshold and raises the largest cut by the same shift, up to 6 dB; a step up has no cut to give back, so the range offers none. The depth question of D153 (6 dB against 3 dB) has no default. The air balance shelf of D162 stays.
- Reason: The eight album masters move the sibilant P99 within 0.84 dB of the other vocal frames (−0.84 to +0.11 dB), and no master shows a keyed cut (D177). The cut of D133 put the sibilant P99 of `master` 4.8 to 6.6 dB under the other vocal frames, and that of `bold` 4.1 to 6.6 dB under them. The tone of `master` lifts the sibilant frames and the other vocal frames by the same filter response, so the balance of the vocal stays without the cut; the mixer's vocal chain set that balance. The air balance shelf returns the air against presence of the mix after the presence moves (D162).
- Alternatives and tradeoffs: A cut at most 3 dB (D153): the sibilant P99 stays about 2 to 3 dB beyond the pairs. A cut that holds each keyed frame at the ratio of the mix in that frame: it cuts the lift of the tone only, which moves the sibilant and the other frames alike. Removing the stage: the Sibilance control then has no key.
- Evidence: the acoustic decisions run. Sibilant P99 against the other vocal frames, on the seven album tracks with a vocal stem and on track-h: `master` −4.80 to −6.56 → −0.21 to −1.45 dB, `bold` −4.14 to −6.64 → −0.12 to −1.66 dB; the album masters −0.84 to +0.11 dB. Air − presence of `master` +0.37 → +0.46 dB (median): the cut had lowered the band from 8 to 10 kHz in the keyed frames. On regression mix 2, 6 to 12 kHz of Full against `essential` −1.89 → +0.32 dB. Every other move changes by 0.1 dB or less, and the versions, names and chosen candidates do not change.
- Rule and code references: `set.deesser.v3`; `deesser::VERSION_CUT_DB`; `adjust::sibilance_shift`.
- Limits and counterexamples: A mix with harsh sibilance keeps it in the default versions; the Sibilance control cuts it on request. D170 compared two de-essers at the same cut, and it did not compare a cut with no cut.
- Verification: `deesser::tests::the_plan_of_a_set_measures_the_key_and_keeps_the_input`, `adjust::tests::a_sibilance_step_cuts_from_a_plan_without_a_cut`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass. No listening result.
- Review condition: A vocal whose sibilance the mixer left harsh; a listening comparison of the Sibilance control at −2 and −4 steps.
- Work reference: TODO.md, "User decisions" and "Listening checks", removed.
- Supersedes / superseded by: Supersedes the cut of `set.deesser.v2` (D133, D153) in the versions of a set. Superseded in part by [D209](#d209-keyed-de-essing-from-the-90-excess-of-the-recording): `set.deesser.v4` replaces the rule `set.deesser.v3` and the Sibilance range. The default of no cut stays in force through [D209](#d209-keyed-de-essing-from-the-90-excess-of-the-recording).

### Updates

- 2026-10-23: [D209](#d209-keyed-de-essing-from-the-90-excess-of-the-recording) replaces the rule `set.deesser.v3` with `set.deesser.v4`. The default of this decision stays in force through [D209](#d209-keyed-de-essing-from-the-90-excess-of-the-recording): with the de-esser switch off, the default, a version of a set keeps the key and the measure and cuts nothing. With the switch on, a version cuts the keyed frames above the 90% quantile of the excess, times the vocal share, by up to 3 dB (`deesser::MAXIMUM_CUT_DB`). `deesser::VERSION_CUT_DB` is removed; `deesser::Plan::from_key` sets the largest cut from the switch. A Sibilance step down moves the threshold to at most 12 dB under the reference excess and the largest cut to at most 12 dB. On 2026-10-24, the default sets of the nine run mixes are byte-identical to v3, in the de-esser v4 run. The test `deesser::tests::the_switch_off_keeps_the_key_and_the_input` replaces `deesser::tests::the_plan_of_a_set_measures_the_key_and_keeps_the_input` of the verification of this decision.

## D182: Bass version from the low end of the album pairs

- Recorded: 2026-10-16.
- Origin: TODO.md: the user decision Low end (D177) and the listening checks Bass version of `master` (D164) and Bass version with an equal-loudness bass cut (D164).
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `set::bass_candidates`, `set::base_ladder`, `set::delivered_base`, `set::WorkerVersion::bass_shelves`, `set::PreparedSet::bass_shelves`, `open::is_bass_shelf`, `open::carries_bass_shelves`, `version_names::designed`, `low_end::bell_maximum_db`, `selection::hold_low_end` and `distributed::render_family_in` as `set.bass-version.v4`. `Spec::bass_step_db` and `set::BASS_STEP_DB` are removed.
- Decision: Where the drums and bass stems hold more than half of the stem power from 40 to 120 Hz (D164), each candidate of `master` carries the two stereo low shelves of `open::plan`: the median sub-against-bass (+1.4 dB) and bass-against-body (+1.2 dB) moves of the album pairs, solved on the spectrum of the mix, each reduced where the mix reaches the largest outcome of the pairs. The low-end hold has no bass step: it counts the shelves as intended moves of the held bands, with bells of up to 2 dB, and its gate checks each section at the floor of D159. The tonal gates of a band under 500 Hz against an unmoved band stay out. When no candidate with the shelves passes, `master` renders the same candidates without them. `bold` builds from the chosen candidate without the shelves. The name Full comes from the shelves (`version_names::designed`).
- Reason: The album masters lift the sub against the bass (median +1.38 dB, P25 to P75 +1.18 to +1.79 dB) and the bass against the body (+1.17 dB, +0.33 to +1.72 dB). The bass step of D164 lifted both held bands toward 1.25 dB above `essential` with two bells and a body cut of up to 2 dB: `master` moved the sub against the bass by −0.38 dB and the bass against the body by +3.67 dB, and its body sat 0.9 to 3.5 dB under the mix at matched loudness. A low shelf lifts the sub more than the bass, and the bass more than the body, as the album masters do, and the loudness match takes the loudness of the shelves from every band.
- Alternatives and tradeoffs: The bass step with a target split per held band: the two held bands do not separate the sub (30 to 80 Hz) from the bass (80 to 250 Hz). The shelves on `bold` as well: the set then has no version without the low end of the pairs. A shelf gain from the pairs' median as the gain: the corner of a shelf lifts part of its region (D178).
- Evidence: the acoustic decisions run. Medians of `master` over the eight album tracks: sub − bass −0.38 → +1.08 dB (+0.80 to +1.38), bass − body +3.67 → +0.77 dB (6 of 8 inside the pair range), presence − body +1.90 → +0.89 dB (4 of 8), low mid − body +1.32 → +0.03 dB (6 of 8), slope −0.20 → +0.14 dB per octave. regression mix 2 Full against `essential` at matched loudness: 500 Hz to 2 kHz −1.96 → −0.54 dB, 6 to 12 kHz +0.21 → +0.04 dB. Each source delivers Full and the four versions.
- Rule and code references: `set.bass-version.v4`; `open::plan`; `low_end::intended_db`.
- Limits and counterexamples: The compressor and peak control take part of the sub move, as on `open` (D178): the median sub move is 0.3 dB under the pairs' median. The hold still uses its body bell in the sections it lifts (D159, TODO.md). A bass part in the `other` stem does not count (D164). The shelves act on the stereo mix; a mix with 2% or more of side power under 100 Hz keeps its side high-pass.
- Verification: `set::foundation_plan_tests::the_bass_shelves_leave_out_the_gates_of_the_held_bands` and `tests/set.rs::a_bass_version_whose_bass_candidates_fail_renders_its_candidates_without_the_shelves`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass. No listening result.
- Review condition: A listening comparison of Full against `open` and the album masters; a track whose bass part is in the `other` stem.
- Work reference: TODO.md, "User decisions" and "Listening checks", removed.
- Supersedes / superseded by: Supersedes `set.bass-version.v3` (D164).

### Updates

- 2026-10-16: Worker plan `mastering.worker-plan.7`. A version of the plan carries `bass_shelves` in place of `bass_step_db`, and a spec has no `bass_step_db`. A worker of 1.9.0 refuses a plan of 1.8.1 (`mastering.worker-plan.6`), so a request on a set of 1.8.1 fails and does not render without the bass step. The app deletes each set of another engine version when it lists the masters of a track (bitneedle-app `listMasters`).

## D183: Low-mid widening from the album pairs

- Recorded: 2026-10-16.
- Origin: TODO.md: the user decision Low-mid width (D177) and the album-pairs item track-h master (D177).
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `width::widening`, `width::LOW_MID_WIDENING_DB`, `set::prepare`, `low_end::next` and `low_end::SPILL_DB` as `set.widening.v4` and `set.low-end.v2`.
- Decision: The band from 120 to 500 Hz of `master` widens by at least the median widening of the album pairs in the band (+4.7 dB), within its own allowance. The bounds of the allowance stay: a P10 window correlation of +0.05 or more, a correlation drop of 0.35 or less and an extra mono loss of 1.5 dB or less. `bold` widens the band to its allowance, at most to the larger of the band above it and +4.7 dB. When the midrange gain of `master` is under 0.5 dB, the bands from 500 Hz up keep the width of the mix, and the side cut of `set.side-cut.v1` stays. The band under 120 Hz keeps the width of the mix in `master` and `bold`; `open` widens it (D178). The low-end hold lifts a held band when the other held band already sits within 0.25 dB of its limit or past it, and that band then rises by at most 0.25 dB.
- Reason: The album masters widen the band from 120 to 500 Hz more than any other band: +4.7 dB at the median, +3.99 to +5.01 dB from P25 to P75. Before this decision, `master` widened the band by +1.58 dB at the median, and 1 of the 8 tracks was inside the pair range. A mono fold-down, a mono speaker and a phone speaker sum the two channels, and a passage with a correlation under 0 cancels there. With the P10 bound of +0.05, nine windows in ten of each band stay above that value. Three album masters end the band with a P10 under +0.05, and each has a cause that is not a widening rule. One channel of the track-h master has the opposite polarity from 0:00 to 0:33. The track-c mix is already at +0.01 in the band (D175). The study track 1 master takes the band from +0.40 to −0.13. In a room, a listener hears little direction under about 80 Hz, and 2.1, club and PA systems feed that band to one subwoofer, which sums the channels. Three album masters take the P10 under 120 Hz under 0: track-h to −0.87, track-g to −0.07 and study track 1 to −0.05.
- Reason, low-end hold: In the last section of track-g, the low-mid widening raises 120 to 250 Hz by 1.9 dB at matched loudness, and the album master raises the band by 1.7 dB there. The hold of `set.low-end.v1` lifted no band past the limit of +0.5 dB, so the bass under that band stayed 0.66 dB short, and each candidate of `master` with the bass shelves failed. A rise of 0.25 dB is half the smallest band change that the set counts as audible.
- Alternatives and tradeoffs: The P10 bound of the band at 0, the gate of the selection: the plan then has no margin for the tone and the hold after it, and regression mix 2 delivers +0.04 from a plan bound of +0.05. The median widening of the pairs under 120 Hz (+4.0 dB) in `master`: `open` gives it, and three album masters cancel there. A hold that counts the widening of each section as an intended move of the held bands: it needs the side power of each section, and the spill bound gives the same result on track-g.
- Evidence: the acoustic decisions run. Side/mid move from 120 to 500 Hz, medians over the eight album tracks: `master` +1.58 → +4.06 dB and `bold` +1.96 → +4.20 dB, each with 4 of 8 tracks inside the pair range against 1 before. The P10 of the band in `master` is +0.07 to +0.70, and track-c keeps the +0.01 of its mix; the album masters end at −0.66 to +0.65. The mono fold-down loss of the band in `master` is 0.35 to 1.23 dB, and 0.29 to 2.36 dB in the album masters. Whole-file side/mid move of `master` +0.53 → +1.89 dB (pairs +1.01 to +2.29 dB). Low mid − body of `master` +0.03 → +0.29 dB, presence − body +0.89 → +1.09 dB. On track-g, `master` keeps the bass shelves with the spill bound: sub − bass +0.66 dB and bass − body +1.36 dB, against −0.64 and 0.00 dB without it. regression mix 1: +1.37 → +4.62 dB in the band, P10 +0.63 → +0.35. Each source delivers four versions.
- Rule and code references: `set.widening.v4`, `set.low-end.v2`; `width::LOW_MID_WIDENING_DB`, `width::OPEN_PATTERN_DB`, `low_end::SPILL_DB`; the window correlation script.
- Limits and counterexamples: The midrange gain of `master` stays as in `set.widening.v3`. On track-d, track-b and track-e it widens each band from 500 Hz up by +3.8 to +4.9 dB, beyond the P75 of the pairs (TODO.md, "Widening (D132)"). The tone width of `width::tone_width_db` counts toward the target, and the delivered band can end under the plan. At matched loudness, the low-mid widening takes about 0.2 dB from the bass, and the hold returns it where a section falls more than 0.5 dB under `essential`.
- Verification: `width::tests::the_low_mids_widen_toward_the_album_median_within_their_allowance`, `width::tests::a_band_past_the_correlation_bound_is_neither_widened_nor_cut` (updated), `low_end::tests::a_band_past_the_limit_takes_a_small_rise_from_the_lift_below_it`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass. No listening result.
- Review condition: A listening comparison in mono and on a phone speaker; a second album of pairs.
- Work reference: TODO.md, "User decisions", Low-mid width, and "Album pairs", track-h master, removed.
- Supersedes / superseded by: Supersedes `set.widening.v3` (D132) and `set.low-end.v1` (D159, D163). `set.widening.v5` (D184) bounds the widening of each band.

## D184: Widening within the album pairs

- Recorded: 2026-10-16.
- Origin: TODO.md: the listening check Widening (D132).
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `width::widening`, `width::PAIR_WIDENING_P75_DB` and `width::PAIR_WIDENING_P90_DB` as `set.widening.v5`.
- Decision: The widening stays in `master` and `bold`. In each band with a side in the mix, the widening of `master` stays within the P75 widening of the album pairs in that band, and the widening of `bold` within the P90. From the band under 120 Hz up, the bounds are +4.2, +5.0, +2.7, +2.1, +2.2 and +2.2 dB for `master`, and +4.7, +5.2, +3.2, +2.4, +2.4 and +3.0 dB for `bold`. The allowance of each band, the low-mid rule of D183 and the bound of 6 dB stay. A band with no side, as in a mono source, takes its width from the decorrelated mid within the bounds of D141.
- Reason: The eight album masters widen every band, by +1.3 to +4.7 dB at the median, so a version without the side gain falls outside the pairs. From 500 Hz up, the pairs widen each band by +0.95 to +2.66 dB from P25 to P75. The midrange gain of `set.widening.v4` widened each band from 500 Hz up of track-d, track-b and track-e by +3.84 to +4.86 dB, and `bold` widened 12 to 20 kHz by up to +8.60 dB. At matched loudness, a larger side lowers the centre of the mix (the lead vocal, the kick, the snare and the bass) against the side, and a mono fold-down removes the added side. The bolder widening of 2026-10-11 (D132) held until the album masters gave a measured distribution; D177 gives it. `bold` takes the P90 side, as its presence does (D180).
- Alternatives and tradeoffs: The median widening of the pairs as the gain of `master`: `open` already has that pattern, so `master` and `open` would have the same width on most mixes. The P75 for `bold` too: `bold` then has the width of `master` where the allowances are large. A bound on the whole-file side/mid: one band can still go past the pairs. The bound on every band with the decorrelated mid included: a mono source has no side for the pairs to bound, and D141 sets its width.
- Evidence: the acoustic decisions run. From 500 Hz up, `master` widens no band past the P90 of the pairs, against three tracks in each band before. Tracks inside the pair range in `master`: 500 Hz to 2 kHz 1 → 3, 2 to 6 kHz 1 → 4, 6 to 12 kHz 2 → 5, 12 to 20 kHz 2 → 2. `bold` widens 12 to 20 kHz by at most +3.16 dB, against +8.60 dB. Medians of `master`: whole-file side/mid +1.89 → +1.66 dB, presence − body +1.09 → +0.89 dB (4 of 8 inside the pair range), bass − body +0.83 → +0.64 dB. Medians of `bold`: presence − body +1.60 → +1.44 dB, air − presence +0.58 → +0.35 dB. Each source delivers four versions.
- Rule and code references: `set.widening.v5`; `width::PAIR_WIDENING_P75_DB`, `width::PAIR_WIDENING_P90_DB`, `width::decorrelated_db`.
- Limits and counterexamples: The tone and peak control change the width after the plan, so a delivered band can end past its bound: track-e `master` ends 0.21 dB past the P75 from 500 Hz to 2 kHz, and `bold` ends up to 0.19 dB past the P90 in four bands. One album sets the bounds. The vocal clarity of a widened version has no measure (TODO.md, "Vocal clarity (D132)").
- Verification: `width::tests::the_widening_of_each_band_stays_within_the_album_pairs`, `width::tests::a_mono_image_takes_its_width_from_the_mid_and_a_stereo_image_does_not`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass. No listening result.
- Review condition: A listening comparison on monitors, in mono and on a phone speaker; a second album of pairs; a mix whose album master widens a band past the P90 of the pairs.
- Work reference: TODO.md, "Listening checks", Widening (D132), removed.
- Supersedes / superseded by: Supersedes `set.widening.v4` (D183) and the bolder widening of `set.widening.v3` (D132, update of 2026-10-11).

## D185: Low-end hold without a body cut

- Recorded: 2026-10-16.
- Origin: TODO.md: the listening check Low-end hold (D159).
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `low_end::bell_gains`, `low_end::rise`, `low_end::loudness_offset_db`, `low_end::shaped_spectrum`, `low_end::next`, `selection::hold_low_end` and `adjust::predicted_hold` as `set.low-end.v3`. `low_end::body_band`, `low_end::BODY_HZ` and `low_end::Step::body_wet` are removed.
- Decision: The hold lifts a held band with its bells only, up to +2 dB, to −0.25 dB against `essential`. The body bell of D159 is removed. The loudness match takes the loudness of the lifts from every band of every section. The hold plans each lift on the spectrum after the tone and the widening of the version (`low_end::shaped_spectrum`). A lift raises its own section by the response of the bell. Every section falls by the loudness that the lifts add to the file, each section weighted by its share of the duration (`low_end::loudness_offset_db`). A second pass lifts the sections that this fall takes under the limit. After a render, the measured fall of the bands that the step left alone scales the loudness of the next step. The limits of the hold, its spill bound (D183) and its gate stay.
- Reason: The hold corrects the bass of a version against `essential`. The body bell also lowered 500 Hz to 2 kHz in each lifted section, by up to 2 dB at 1 kHz: the region of the lead vocal and of the fundamentals of the lead instruments. That cut was a second tonal change, up to four times the 0.5 dB step that the set counts as audible, and it varied from section to section with the hold. Without it, a lift in one section of thirteen raises that section by the response of the bell and the loudness of the file by about a thirteenth of the loudness of the bell. The plan of D159 took the loudness of the whole file from each lift beyond the full body cut, so a lift without the body cut came out about twice its plan, and the loudness of the lifts took sections near the floor under it.
- Alternatives and tradeoffs: A body cut of at most 0.5 dB: the hold then needs both mechanisms, and the cut stays a section-dependent change. A section gain that keeps the loudness of the section: the level of the section then moves against the sections around it. A third render of the hold: track-e `bold` needs it to keep its first candidate, at the cost of one render more for each candidate that ends near the floor.
- Evidence: the acoustic decisions run. The spread of the body of `master` over the sections falls from 0.80 to 0.31 dB on track-h, from 1.16 to 0.46 dB on track-d and from 1.96 to 0.54 dB on track-b. On the ten sources it is 0.04 to 0.74 dB after in `master` and `bold`, except 2.69 and 2.75 dB on study track 1, where other moves of the versions set it. The album masters move the body over their sections by 0.30 to 1.47 dB, and by 2.67 dB on track-h. Each `master` takes the same candidate as before. Medians of `master`: presence − body +0.89 → +0.76 dB (5 of 8 inside the pair range), bass − body +0.64 → +0.61 dB. Each source delivers four versions.
- Rule and code references: `set.low-end.v3`; `low_end::bell_gains`, `low_end::loudness_offset_db`, `low_end::shaped_spectrum`; the section script, for the body of each section.
- Limits and counterexamples: track-e `bold` takes its third candidate, the vivid tone with the widening of `master`: its first two candidates end 0.52 dB under `essential` in their worst section, 0.02 dB past the floor. The duration share of a section stands in for its share of the integrated loudness; the measured fall corrects it after the first render. study track 1 `essential` takes the name Bright: without the body cut, `master` and `bold` have less treble against their body.
- Verification: `low_end::tests::the_loudness_match_takes_the_lift_from_every_section`, `low_end::tests::the_fall_of_the_bands_left_alone_scales_the_loudness_offset`, `low_end::tests::a_bass_shelf_raises_the_loudness_that_a_bell_adds`, `low_end::tests::the_hold_applies_only_in_failing_sections` (updated). `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass. No listening result.
- Review condition: A listening comparison of a held section against its neighbors; a track whose hold lifts most sections.
- Work reference: TODO.md, "Listening checks", Low-end hold (D159), removed.
- Supersedes / superseded by: Supersedes `set.low-end.v2` (D159, D183).

## D186: Open widening within the window bound of every version

- Recorded: 2026-10-16.
- Origin: TODO.md: the listening check Album pairs (D177, D178), the low-mid width of `open`; the finding of D183 on the three album masters that end a low-mid band under 0.
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `width::OPEN_WINDOW_CORRELATION` as `set.open.v3`, and in `set::Summary::open_withheld`.
- Decision: The Open widening keeps the P10 window bound of the other versions: 0, and +0.05 with the margin of the plan. The bound was −0.25. The rest of `set.open.v2` stays: a set delivers `open` only where both bands under 500 Hz admit at least 1 dB of the Open widening. The analysis of a set gives the reason when it delivers no `open` (`open_withheld`).
- Reason: A mono fold-down, a mono speaker and a phone speaker sum the two channels, and a passage with a correlation under 0 cancels there; `open` is one of the delivered versions. D178 set −0.25 because three album masters end a low-mid band under 0. D183 found the causes: one channel of the track-h master has the opposite polarity from 0:00 to 0:33, the track-c mix is already at +0.01 in the band (D175), and the study track 1 master takes the band from +0.40 to −0.13. These are no evidence for a widening under 0.
- Alternatives and tradeoffs: The bound of −0.25 for `open` alone: `open` is then the one delivered version that can cancel in mono. An `open` on every mix at the widening that the bound allows: on track-c and regression mix 2 it widens the band from 120 to 500 Hz by 0 and 0.7 dB, under the condition of the version.
- Evidence: the acoustic decisions run. track-c and regression mix 2 deliver no `open`: the band from 120 to 500 Hz admits 0.0 and 0.7 dB of the Open widening, under 1.0 dB. On track-g, study track 1 and track-h, the lowest P10 of a band of `open` rises from −0.25, −0.15 and −0.11 to −0.01 or more, at a smaller widening of their low bands; on regression mix 1, 6 to 12 kHz of `open` widens by +0.71 dB, against +1.47 dB, at a P10 of +0.05. Four sources keep their `open` unchanged. Median whole-file side/mid move of `open` +2.14 → +2.11 dB, with 6 of 7 tracks inside the pair range, against 4 of 8.
- Rule and code references: `set.open.v3`; `width::OPEN_WINDOW_CORRELATION`, `width::WIDEN_WINDOW_CORRELATION`, `width::OPEN_FIT_MINIMUM_DB`; `set::Summary::open_withheld`.
- Limits and counterexamples: The album masters themselves take the P10 of a band under 0 on four of eight tracks. The reason of a withheld `open` is in the analysis of a native set; the worker plan of the cloud set does not carry it.
- Verification: `width::tests::the_open_allowance_keeps_the_window_bound_of_every_version`, `width::tests::the_open_version_fits_a_narrow_low_end_and_not_a_wide_one`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass. No listening result.
- Review condition: A listening comparison of `open` in mono and on a phone speaker; a mix whose low end is already wide.
- Work reference: TODO.md, "Listening checks", Album pairs (D177, D178).
- Supersedes / superseded by: Supersedes `set.open.v2` (D178).

## D187: DC offset in the source condition

- Recorded: 2026-10-16.
- Origin: TODO.md: the engine item DC offset (D142).
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `source_condition::dc_offset_dbfs` and `SourceCondition::dc_offset_dbfs` as `set.source-condition.v3`.
- Decision: The source condition reports the DC offset of the left and the right channel: the mean of the channel over the recording against full scale, in dBFS, at least −200 dBFS. The report changes no processing. The chain has no DC filter.
- Reason: A DC offset takes headroom from one polarity of the peaks and passes through the low shelves of the tone at their gain at 0 Hz. On the ten sources of the regression set, the largest offset is −69.2 dBFS (regression mix 2); it takes 0.003 dB of headroom, and the low shelves of `master` raise it by about 3 dB. The album mixes measure −91 to −119 dBFS. A DC filter would add a phase shift under 20 Hz to every source for no measured gain.
- Alternatives and tradeoffs: A DC filter for a source over a threshold: no source of the set comes near one. A DC filter on every source: a phase change of the low end of each mix.
- Evidence: the acoustic decisions run.
- Rule and code references: `set.source-condition.v3`; `source_condition::DC_FLOOR_DBFS`.
- Limits and counterexamples: The report needs a reader: the MASTER screen does not show it. A recording from a faulty converter can carry an offset of −40 dBFS or more.
- Verification: `tests/source_condition.rs::the_dc_offset_of_each_channel_is_reported`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass.
- Review condition: A source with an offset over −50 dBFS.
- Work reference: TODO.md, "Engine", DC offset (D142), removed.
- Supersedes / superseded by: Extends `set.source-condition.v2` (D142, D175).

## D196: Reference render shared between families

- Recorded: 2026-10-16.
- Origin: Engineering proposal: TODO.md, "Share the Light reference measurement between the cloud families". The `master`, `bold` and `open` families each rendered the Essential reference again.
- Status: Accepted for hosts whose families share cores. Implemented in `selection::Reference`, `selection::Reference::record`, `selection::Reference::from_record`, `distributed::Store::publish_reference`, `distributed::Store::reference`, `distributed::render_family_in`, `distributed::REFERENCE_SCHEMA` and the local family example. The family Lambda renders its own reference.
- Decision: The Essential family gives the measurements of its first render to its host as the reference of the other families of the job (`distributed::Store::publish_reference`). The record (`mastering.reference-render.1`) holds what the gates, the warm start and the low-end hold read: the drive, the loudness, the prefix loudness, the restoring gains, the mean limiter reduction, the P90 attack loss, the six whole-file stereo bands and the bass sections. Each number keeps its 64 bits. The record names the source PCM and the SHA-256 of the reference spec. A family with a reference spec takes the record from its host (`distributed::Store::reference`). It renders the reference itself when the host gives no record, or a record of another spec, of another source or without a reference. The Essential family publishes the record before it writes its WAV. `family_local` passes the record through a directory. The family Lambda gives no record.
- Reason: The reference render was about 40% of the main-thread time of the `master` family on regression mix 2. The first candidate of a family starts from the drive, the loudness and the restoring gains of the reference, so it cannot start before the reference is known. The Essential family makes the same render as its first candidate, and it starts at the same time as the other families. The record arrives when the family would have finished its own reference render. On a host whose families share cores, the waiting families give their cores to the Essential family. A family Lambda has its own vCPUs and Lambda bills its wait, so on Lambda the record saves no time and no cost. At 29.9 minutes the Essential family takes about 140 to 300 s on Lambda, and a family Lambda that waits for the record would add that wait to its time when the Essential family fails.
- Alternatives and tradeoffs: The Essential family starts the other families after its first render: their Lambda time falls by the reference render, but the critical path of the job grows by the decode of a family. A family that renders the first candidate while it waits: the warm start of the candidate needs the reference, so the output would change. A cache of the shaping before the restoring filters for the first candidate of a family: at most the shaping of one prefix. The planner renders the reference: the critical path grows by the reference render.
- Evidence: the software speed run. On regression mix 2, one family at a time on an M1, the `master`, `bold` and `open` families take 25.3, 25.9 and 15.8 CPU-s against 44.9, 46.0 and 37.3; the four families take 88.6 against 149.6 CPU-s. With the four families at once on two threads each, the families phase took 34.8 s against 45.5 s, because the waiting families left their cores to `essential`.
- Limits and counterexamples: The saving is CPU time. A family whose Essential family fails waits for the whole wait of its host (600 s in `family_local`) before it renders the reference. The `work` counts of a family that takes the record leave out the reference render.
- Verification: Twelve runs of the cloud path of regression mix 2 with `family_local` give the WAVs, plan, manifest and family JSON of 1.8.1, apart from `seconds` and `work`. `tests/set.rs` compares a family that takes the record with a family that renders the reference. The suite passes.
- Review condition: A host whose families share cores, such as a browser that renders the families beside each other. A form that saves Lambda time (TODO.md, "Reference render of the cloud families"). A reference that reads a measurement outside the record.
- Work reference: TODO.md, "Cloud MASTER".
- Supersedes / superseded by: None. Changes the alternative "Sharing the Light reference measurement" of D147.

## D197: Medium presets: Streaming, Club, Radio and Mono

- Recorded: 2026-10-17.
- Origin: TODO.md, "Advanced presets and ADJUST", the medium presets after Vinyl. The user decisions of 2026-10-12: presets for the medium and the style drive the ADJUST panel, a preset exists where the default versions do not cover the result, and the width of the mix stands.
- Basis: Acoustics, measurement, the published limits of each medium and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `adjust::Preset`, `adjust::medium`, `adjust::club_lufs`, `adjust::folded`, `adjust::medium_change`, `adjust::offers`, `adjust::preset_candidates`, `adjust::finalize` (`medium`), `adjust::preview_preset` and `width::SideGain::mid_db`, as `adjust.streaming.v1`, `adjust.club.v1`, `adjust.radio.v1` and `adjust.mono.v1`. `master adjust --preset`, the cloud action `adjust` (`preset`) and the WASM `Preview` (`{"preset": NAME}`) take the keywords `streaming`, `club`, `radio` and `mono`. The ADJUST panel of bitneedle-app shows a preset from its entry in `PRESETS`, and each of the four presets has an entry. Each Club candidate carries `selection::Gates::under_own_point` (check `selection.own_point`). The site runs the presets with engine 1.11.0, and the cloud runs 1.9.0.
- Decision:
  - Streaming: the target is −14 LUFS for a version louder than −14 LUFS, and the ceiling is −1 dBTP. Each other stage of the parent stays.
  - Club: the target is the audibility point of the set (D131), moved by the change of the ceiling, less 0.5 LU. It is at most −6.5 LUFS and never under the target of the parent. The ceiling is −1 dBTP. The low end stays. Each candidate stays at or under the audibility point of its own prefix, with the restoring gains of its render. A candidate above that point gives way to the next share.
  - Radio: the target is −23 LUFS and the ceiling −1 dBTP, with the fold of Mono. The preset adds no FM processing.
  - Mono: the fold. In each band that the parent widens, the side gain and the decorrelated mid return to 0 dB, the side/mid of the mix. A zero-phase mid gain gives the mid the side power that the widening added, so the stereo power of the band stays.
  - Streaming has one candidate. Club takes the full rise of the level, three quarters and half of it. Radio and Mono take the full fold, three quarters and half of it. Each candidate keeps the ceiling, and Streaming and Radio keep their published level, as delivery conditions.
  - `adjust::range` offers a preset where its predicted change reaches 0.5 dB or 0.5 LU in a measure that serves its medium: the level or the ceiling for Streaming, the level for Club, the mono sum of a band at matched loudness for Mono, and each of them for Radio (`adjust::offers`). A request for a preset that the version does not offer is a result with the reason `outside-range`.
  - The result names the parent and the preset, for example "Full · Streaming". Its `medium` report gives the delivered level, ceiling and true peak with those of the parent, and the change of the PLR, of each band at matched loudness, of the width and of the work of peak control. With the fold, it gives the predicted change of the mono sum of each band and the measured mono fold-down loss of each band of the mix and of the result. The description states the level, the peaks, the level change against the parent and the bands whose mono sum rises.
  - A fold that moves the mono sum of a band by 0.5 dB or more makes the result distinct from its parent in the duplicate screen: the sound of a version record holds no mono sum.
- Reason, Streaming: Spotify plays music at −14 LUFS by default and turns a louder master down. It asks for a true peak under −1 dBTP at −14 LUFS, and under −2 dBTP for a louder master (Spotify for Artists, "Loudness normalization"). Apple asks for at least 1 dB of headroom (Apple, "Apple Digital Masters"). AES TD1008 (AESTD1008.1.21-9) asks for −1 dBTP or lower at the input of a lossy codec, and plays the loudest track of an album at −14 LUFS. The default versions play at −9.9 to −14.1 LUFS with a ceiling of −0.5 dBTP (D155, D179). Their decoded true peaks reach +0.58 dBTP with AAC at 256 kbit/s and +1.80 dBTP with Opus at 128 kbit/s (D155, 2026-10-16).
- Reason, Club: Streaming services and broadcasters normalize to a published level. A DJ sets the level of each track with the channel gain of the mixer, and club playback has no published loudness level, so a louder file plays louder at the same gain. The audibility point is the loudness at which the masked disturbance of peak control reaches its masking threshold (D131). Peak control works against the ceiling, so a ceiling 0.5 dB lower lowers the point by 0.5 LU. The margin is the step of 0.5 LU of D131. The subwoofers of a large system play the sum of the channels under the crossover: the subwoofers of a common PA range cross over at 80 or 100 Hz and take a summed mono feed (manufacturer documentation). The band under 120 Hz of each delivered version of the regression set loses 0.00 to 0.10 dB in that sum, so the low end needs no change. The ceiling of −1 dBTP is the codec limit of AES TD1008. A decoded peak of a lossy copy above 0 dBFS clips in a player without loudness normalization (D155).
- Reason, Radio: EBU R 128 s3 ("Loudness in Radio", 2023) produces each radio programme at −23.0 LUFS with a true peak of −1 dBTP or lower, and normalizes legacy programmes to −23.0 LUFS before play-out. EBU Tech 3401 keeps the peak-to-loudness ratio (PLR) in production and adapts it in distribution. EBU Tech 3344 puts the FM pre-emphasis limiting and the 15 kHz low-pass in the distribution stage. An FM stereo transmission carries M, half the sum of the channels, in its main channel, and a mono receiver plays M (ITU-R BS.450-4, § 2.1.2.1). Radio therefore takes the fold.
- Reason, Mono: A single speaker plays the mid. A side gain changes the side alone (`width::SideGain::apply`), so a side cut, keyed (D176) or static, leaves the mono sum as it is. The widening of a version raises the side of a band, and the loudness match lowers the whole file by the added power. A single speaker then plays the band lower against the other bands by the side power that the widening added. The fold gives the mid that power. The stereo tone stays, and the image of the band returns to the side/mid of the mix (user decision 4 of 2026-10-12).
- Alternatives and tradeoffs:
  - Streaming at −16 LUFS, the Sound Check level of Apple Music and the music mean of AES TD1008: Spotify raises a softer master only within 1 dB of headroom, so a master at −16 LUFS and −1 dBTP plays 2 LU under one at −14 LUFS.
  - Streaming at the level of the parent with a ceiling of −2 dBTP, the Spotify limit for a louder master: peak control takes 1.5 dB more, and each service turns the level down.
  - Club at the audibility point (D131, `set.audible-loudness.v1`): regression mix 1 `master` sits 2.27 LU under its own point at its target (D131, 2026-10-16), and the Club level would pass that point.
  - Club with a side high-pass at the crossover: the sum of the channels keeps the low end of each version within 0.10 dB.
  - Club with an equal-loudness compensation for the venue limit of the WHO, 100 dB LAeq over 15 minutes (WHO, "Global standard for safe listening venues and events", 2022), about 25 phon above the monitoring level of 75 phon (`broad_tone::REFERENCE_PHON`): the ISO 226 contours give −9.2 dB from 40 to 125 Hz. A venue sets the response of its system at its own level.
  - Radio at ATSC A/85 (−24 LKFS, a true peak of −2 dBTP): A/85 gives the loudness of US television, and EBU R 128 s3 that of radio.
  - Mono and Radio with the keyed cut of D176: a side cut leaves the mono sum unchanged.
  - Mono as the widening removed without the mid gain: the stereo power of each widened band falls by the added side, and the mono sum keeps its balance.
  - Mono to the width of the mix with its own side folded: the image narrows past the mix.
- Evidence: the presets run, on the 38 versions of the ten sources of the regression set and on 36 results of six of them.
  - Offers: each version offers Streaming and Radio, and Club with +1.75 LU, or +1.97 LU on track-d. Mono is offered on 8 versions: the `master`, `bold` and `open` of track-h and track-g, from 120 to 500 Hz, and the `bold` of track-d and track-b, from 500 Hz to 2 kHz. The widening of each other version moves the mono sum of each band by less than 0.5 dB.
  - Streaming, 9 results on 4 tracks: −13.97 to −14.00 LUFS and −1.00 to −2.12 dBTP, and FFmpeg reads −14.0 LUFS and −1.0 to −2.1 dBTP. The PLR rises by 0.23 to 3.51 dB. Each band stays within 0.05 dB at matched loudness, and the mean limiter reduction falls by 0.009 dB or less. On track-h, track-g, track-e and track-d, `master` decodes to −0.3 to +1.2 dBTP with AAC at 256 kbit/s, Vorbis at 320 and 160 kbit/s and Opus at 128 kbit/s, and its Streaming result to −0.3 to −2.0 dBTP.
  - Club, 9 results on 4 tracks. Eight take the full rise: 1.74 to 1.76 LU louder at −1.00 dBTP, and 0.90 to 2.41 LU under the own audibility point of the result. On regression mix 2 `master`, the full rise and three quarters of it fail the low-end hold, and half of it delivers +0.87 LU, 0.72 LU under the own point. The mean limiter reduction rises by 0.009 to 0.106 dB, and each band stays within 0.30 dB at matched loudness.
  - Radio, 11 results on 6 tracks: −22.99 to −23.00 LUFS with a true peak of −6.64 to −10.93 dBTP, so peak control does no work. The PLR rises by 2.54 to 6.54 dB, and each band stays within 0.17 dB at matched loudness.
  - Mono, 7 results on 4 tracks: the level and the peaks of the parent, and each band within 0.14 dB at matched loudness. In the band of the largest gain, the mono sum rises by +0.64 to +0.84 dB, against +0.57 to +0.85 dB predicted. In each folded band, the result loses 0.00 to 0.22 dB less in the mono fold-down than the mix. The width from 500 Hz to 12 kHz falls by up to 3.55 dB.
  - The band under 120 Hz of each delivered version loses 0.00 to 0.10 dB in the sum of the channels.
- Rule and code references: `adjust.streaming.v1`, `adjust.club.v1`, `adjust.radio.v1`, `adjust.mono.v1`; `adjust::STREAMING_LUFS`, `adjust::CODEC_CEILING_DBTP`, `adjust::RADIO_LUFS`, `adjust::RADIO_CEILING_DBTP`, `adjust::CLUB_MARGIN_LU`.
- Limits and counterexamples: No listening result. One album of eight tracks and two other sources. In normalized playback, the level change of Streaming and Radio disappears: at matched loudness, the bands change by 0.17 dB or less and the mean limiter reduction by 0.020 dB or less. Each default version already sits 2.27 to 4.92 LU under its own audibility point (D131, 2026-10-16). The Club level comes from the audibility point of `essential`, and each Club result stays 0.72 to 2.41 LU under its own point (TODO.md, "Club level from the own audibility point"). The fold narrows the image of each widened band for a stereo listener. The fold comes from the long-term spectrum. The mono sum of a band follows the prediction within 0.13 dB in Mono, and within 0.17 dB in Radio, whose peak control also changes. Apple Music (Sound Check, −16 LUFS) and YouTube (−14 LUFS) publish no normalization level; the levels in this decision are those that Spotify and AES TD1008 publish. A radio station normalizes a commercial master to −23 LUFS itself (EBU R 128 s3), so the Radio file differs from that normalization by the fold and by peak control at rest.
- Verification: `adjust::tests::each_preset_keyword_parses_back`, `streaming_takes_the_level_and_the_ceiling_of_the_services`, `club_takes_the_audibility_point_at_its_ceiling`, `the_fold_keeps_the_band_power_and_returns_the_side_of_the_mix`, `mono_and_radio_fold_where_the_mono_sum_changes`, `a_medium_description_states_the_level_the_peak_and_the_fold`, `the_preview_of_a_preset_takes_its_level_in_the_drive` and `the_range_gives_the_first_share_of_each_step` (updated); `width::tests::a_mid_gain_raises_the_mono_sum_of_its_band_and_keeps_the_side`. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass. The Lambda crate passes `cargo fmt --check` and `cargo clippy --release --all-targets -- -D warnings`, and the WASM crate `cargo check`. The nine sets of the 1.9.0 regression set give the same WAVs and JSON as engine 1.9.0. No listening result.
- Review condition: A platform document with other limits; a club playback chain that normalizes loudness; a version whose Club result passes its own audibility point; a listening comparison on a single speaker.
- Work reference: TODO.md, "Advanced presets and ADJUST", the medium presets after Vinyl, removed.
- Supersedes / superseded by: Extends D176. Uses D131, D155 and D179.

### Updates

- 2026-10-19: Engine 1.10.0 (D202 to D204). Measured in the presets run, on the 34 versions of the nine sources of the regression set and on 36 delivered results of six of them.
  - Offers: each version offers Streaming and Radio. Club raises `essential`, `master` and `open` by 1.75 LU, or 1.90 LU on track-d. `bold` plays 0.5 LU under the point (D203), at or above the Club level, so it does not offer Club. Mono is offered on 11 versions.
  - Own point of Club: the Club level comes from the point of `essential`. On 1.10.0 the shelves and the sub lift give `master` and `open` more low end, and a version with more low end reaches its own point sooner. Three quarters of the Club rise on regression mix 2 `master` were 0.13 LU above the own point of the result. Each Club candidate now stays at or under the point of its own prefix. regression mix 2 `master` delivers half of the rise, +0.87 LU, 0.49 LU under its own point. The other 6 Club results are 0.90 to 1.65 LU under their own points.
  - Streaming, 11 results: −13.99 to −14.00 LUFS and −1.00 to −2.23 dBTP. Each band of `master` and `open` stays within 0.07 dB at matched loudness. A band of `bold` rises by up to 0.16 dB, and its largest limiter reduction falls by 1.94 to 2.78 dB.
  - Radio, 11 results: −22.99 to −23.00 LUFS and −4.31 to −10.96 dBTP, each band within 0.22 dB. Mono, 7 results: each band within 0.12 dB, and the mono sum of the band of the largest gain within 0.08 dB of the prediction.
  - The fix of D204 of 2026-10-19 keeps the shelves of the parent in each result. Before the fix, the Streaming result of track-h `master` had 0.20 dB more under 120 Hz than its parent at matched loudness.

## D198: No style preset

- Recorded: 2026-10-17.
- Origin: TODO.md, "Advanced presets and ADJUST", Style presets: a style preset only where the default versions leave a gap, decided from the delivered versions and the pair prior of D177.
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Accepted. Implementation: none.
- Decision: The ADJUST panel offers no style preset. The default versions are `essential`, `master`, `bold` and `open` (D178 to D186).
- Reason: A gap on a track is a move of its album master that no delivered version reaches within 0.5 dB, the smallest change that the set counts as audible. The gaps fall in four groups, and no group asks for a style preset:
  - Low mid − body, presence − body and air − presence: a gap on 2 of 8 tracks each, in opposite directions. The album masters move these regions up on one track and down on another. A preset would take one direction of the decisions of each track (D112).
  - Bass − body and sub − bass: a gap on 4 and 3 of 8 tracks, the album master 0.52 to 1.14 dB above every version. `master` moves bass − body by +0.61 dB and sub − bass by +1.01 dB at the median, against shelf moves of up to +1.2 dB and +1.4 dB (D182). The chain takes part of the planned move (TODO.md, "Sub lift"), and that item answers the shortfall in the versions. With the planned moves delivered, the gap stays on track-h and track-g in bass − body and on track-c in sub − bass, where the album master is above the P75 move of the pairs.
  - Width: a gap from 120 to 500 Hz on 5 tracks, with the album master wider on 4 of them, and a gap on 1 to 3 tracks in each other band, with the album master wider. Each delivered version keeps the P10 window correlation of each band at 0 or more (D183, D186). The album masters take a band under 0 on four tracks, and those passages cancel in mono.
  - Level, crest and attack: the album master is 0.55 to 1.23 LU louder on 3 tracks and loses 0.67 to 1.65 dB more attack at P90 on 7 tracks. Loudness normalization removes the level (D179). At the audibility cap, a denser render differs by 0.05 dB or less at matched loudness in each measure of the set (D135). The Club preset gives the louder level for unnormalized playback (D197).
- Alternatives and tradeoffs: A Fuller low end preset at the P75 moves of the pairs (bass − body +1.72 dB, sub − bass +1.79 dB): it answers a shortfall of the versions with a request, and it lifts the four tracks whose album masters move bass − body by +0.82 dB or less. A Dense preset: D135. A Darker preset at the P25 presence move of the pairs (−0.66 dB): the gaps in presence change sign between tracks.
- Evidence: the presets run. The pair measurement of the acoustic decisions run, whose 34 WAVs equal those of the 1.9.0 regression set by SHA-256.
- Rule and code references: the pair measurement script.
- Limits and counterexamples: One engineer and one album. The gap test uses the moves of the whole file; a section of a track can differ.
- Verification: Measurements only.
- Review condition: The Sub lift item (TODO.md); a second album of pairs; a gap on more than half the tracks in one direction.
- Work reference: TODO.md, "Advanced presets and ADJUST", Style presets, removed.
- Supersedes / superseded by: None.

### Updates

- 2026-10-19: The review after the sub lift (D204), on the pairs of the 1.10.0 regression set. Sub − bass has a gap on 1 of 8 tracks (track-c, +0.57 dB) and bass − body on 2 of 8 (track-h +0.81 dB, track-d +0.75 dB). The P90 attack loss has no gap. The width has a gap on 4 of 8 tracks in each band from 20 Hz to 2 kHz, with the album master wider. No gap falls on more than half the tracks in one direction. The decision stands.

## D199: Presets as the successor of the controls

- Recorded: 2026-10-17.
- Origin: User direction of 2026-10-12: presets for the medium and the style drive the ADJUST panel, and the low-level dials go. TODO.md, "Advanced presets and ADJUST": record the presets as the successor decision of D171.
- Status: Accepted. Implemented as the presets of D176 and D197. The dials of D171 stay in the engine, the cloud action and the app until the presets or the versions cover them (TODO.md, "Advanced presets and ADJUST").
- Decision: The ADJUST panel offers presets. A medium preset sets each amount from the measurements of the mix and the published limits of its medium: Streaming, Club, Radio and Mono (D197). Mono-safe is withdrawn (D176), and Vinyl waits for the limits of a cutting room (TODO.md). A style preset exists only where the default versions leave a gap; none does (D198). Each result states its measured change against its parent. The dials of D171 (Bass, Presence, Air, Sibilance, Loudness, Width, Compression and Peak level) go when the presets or the versions cover them.
- Reason: Dials make the user the engineer. Presets keep the measurements and the reasons of the engine in front of the user.
- Alternatives and tradeoffs: Dials beside the presets (D171): the ranges of the dials have no listening result, and a dial asks the user for an amount without a measurement behind it. Presets with a dial for the amount: the amount is the decision of the preset.
- Evidence: D176, D197, D198.
- Rule and code references: `adjust::Preset`; `adjust.steps.v1` stays until the removal.
- Limits and counterexamples: No preset covers bass, presence, air, sibilance, compression or a wider image. The ceilings of the presets and the levels of Streaming and Radio are fixed values. The dials stay for these until a preset, a version or a decision covers them (TODO.md).
- Verification: D176, D197.
- Review condition: The removal of the dials (TODO.md).
- Work reference: TODO.md, "Advanced presets and ADJUST".
- Supersedes / superseded by: Supersedes the direction of D171. The controls of D171 stay implemented until their removal.

## D200: Presence of a captured recording from its deficit

- Recorded: 2026-10-17.
- Origin: TODO.md, "Captured recordings", stage 4 (D134); the capture plan, stage 4. stress recording 1 is 8.46 dB under the equal-loudness balance, and the presence move of `master` stops at 2 dB.
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `capture::presence_move_maximum_db`, `capture::PRESENCE_BELL_MAXIMUM_DB`, `capture::Capture`, `broad_tone::presence_balance`, `broad_tone::analyze`, `broad_tone::master`, `broad_tone::vivid_tone`, `set::bold_candidates`, `SourceCondition::device_capture` and `source_condition::reported_device_capture` as `set.capture-presence.v1`.
- Decision: For a source that `SourceCondition::capture` reports (D146), the static tone of `master` moves presence (2 to 5 kHz) against body (500 Hz to 2 kHz) up to the darkest album master (−9.10 dB) where the recording is darker. A capture inside the album outcomes moves by at most the P75 move of the pairs (+0.94 dB). No capture moves past the equal-loudness balance (−3.30 dB). The bound of 2 dB of a mix does not apply. The presence balance bell and the summed static response of `master` go up to 6 dB, the largest presence bell of `bold`. `bold` keeps its step of 1 dB and its bound: the bound of `master` plus 0.48 dB. A mix keeps `set.presence-balance.v2`.
- Reason: The bound of 2 dB keeps the balance that a mixer chose (D180). A device set the balance of a captured recording, and no person balanced it at its level (D146). The eight album masters of D177 give the range of outcomes. A capture darker than every album master moves to the end of that range, so its move follows its deficit to the equal-loudness balance one for one beyond 5.80 dB.
- Alternatives and tradeoffs: The equal-loudness balance as the target: +8.46 dB on stress recording 1, past a DAW mastering assistant (+2.97 dB) and past the video master of the user (+1.6 dB on the excerpt). The median album master (−4.83 dB) as the target: +6.93 dB on stress recording 1. A fixed share of the deficit: no measure sets the share. The bound of a mix: the presence of stress recording 1 stays 2.66 dB under every album master.
- Evidence: the capture stages run. Dry run without stems: the bound changes on stress recording 1 (2.00 → 2.66 dB, the excerpt 2.00 → 2.78 dB), stress recording 6 (2.00 → 3.86 dB) and stress recording 4 (2.00 → 2.02 dB); stress recording 2, stress recording 3, stress recording 5, stress recording 7 and stress recording 8 keep their bound; the presence balance bell of stress recording 6 is 5.28 dB. The DAW comparison uses the Mastering Assistant of Logic Pro at its neutral setting. The mastering assistant moves the presence against body of the dark captures by +1.98 to +4.26 dB, to −10.38 to −6.09 dB. Renders against 1.9.0, without stems and, for the stress recording 1 excerpt and stress recording 8, with stems: presence against body of `master` of the stress recording 1 excerpt −10.24 → −9.01 dB with stems and −9.78 → −9.00 dB without, of `bold` → −8.56 dB (bound −8.62 dB); 2 to 6 kHz at matched loudness of `master` +0.60 → +1.90 dB and +2.78 → +3.58 dB, of `bold` +3.28 → +4.09 dB. The video master of the user raises that band by about 4.2 dB, the mastering assistant by 6.33 dB. Without stems the excerpt now delivers `bold`, a duplicate of `master` in 1.9.0. stress recording 6 delivers `master` at −11.20 dB, which no candidate of 1.9.0 passed (its tonal gate measured 0.58 against 0.81 dB). The other captures keep their presence. Each chosen candidate passes every check, and each version peaks at −0.5 dBTP or less. Every version of 1.9.0 and of the stage raises the band above the codec cutoff by +6.2 to +31.7 dB against the source at matched loudness, 58.6 to 86.4 dB under 1 to 4 kHz; the stage changes it by −1.04 to +0.78 dB against 1.9.0, by 0.5 dB or more only on the stress recording 1 excerpt (+0.65 to +0.78 dB, 71 dB or more under 1 to 4 kHz).
- Criterion in place of the listening review (pass criterion 7): the stage moves each recording that it acts on toward the range of the album masters, by 0.5 dB or more (`selection::DISTINCT_DB`, over the detectable level increment of 0.41 dB of Miller 1947), and not past the darkest album master or the equal-loudness balance of ISO 226:2003. The comparisons are at matched integrated loudness (ITU-R BS.1770), the condition of loudness-normalized playback. The stage acts on the stress recording 1 excerpt (+0.78 and +1.23 dB against 1.9.0, to −9.00 and −9.01 dB) and on stress recording 6 (+2.54 dB against its `essential`, to −11.20 dB), and both meet it; on stress recording 4 the bound rises by 0.02 dB.
- Rule and code references: `set.capture-presence.v1`; `capture::PRESENCE_RULE`; `broad_tone::PRESENCE_OUTCOME_MINIMUM_DB`; the capture stages example.
- Limits and counterexamples: One album sets the outcome range, and its masters are studio mixes of a band. Pass criterion 3 fails as written for every capture version of 1.9.0, and the stage adds up to +0.78 dB to that band; the plan gives no bound for the level of the band (TODO.md). stress recording 6 reaches −11.20 dB, short of its plan, and it is a vocal and a piano. The video master of the user moves the excerpt less (+1.6 dB). Most captures have no stems on this computer, so the lead bells of the vocal are not measured on them. The `open` version keeps its own tone.
- Verification: `capture::tests::a_dark_capture_moves_to_the_darkest_album_master`, `broad_tone::tests::a_dark_capture_takes_a_larger_presence_move_than_a_mix`, `set::foundation_plan_tests::the_bold_of_a_dark_capture_steps_within_the_bound_of_a_capture`, `tests/source_condition.rs::a_reported_condition_gives_the_device_capture_of_the_condition`, `source_condition::device_capture_tests`. The nine sets of the regression set render the WAVs and JSON of 1.9.0, and after the merge into 1.10.0 those of 1.10.0. No listening result.
- Review condition: A capture that an engineer mastered; a listening comparison on stress recording 1 and stress recording 6.
- Work reference: TODO.md, "Captured recordings", stage 4, removed.
- Supersedes / superseded by: Extends D134, D146 and D180 for a captured recording.

### Updates

- 2026-10-19: The stage is merged into engine 1.10.0 (D202 to D204) and measured again. The dry run is that of 1.9.0. On the stress recording 1 excerpt, presence against body and 2 to 6 kHz of `master` and `bold` are those of 1.9.0 within 0.01 dB. stress recording 6 delivers `master` at −11.05 dB, +2.60 dB against its `essential`. On 1.10.0 no candidate of that `master` passes the tonal gate (0.68 against 0.81 dB). Each chosen candidate passes every check. Against the same version of 1.10.0, this stage changes the band above the codec cutoff by 0.5 dB or more only on the stress recording 1 excerpt. The changes there are +0.66 to +0.78 dB, and the band is 71 dB or more under 1 to 4 kHz. The nine sets of the regression set render the WAVs and JSON of 1.10.0.
- 2026-10-19: Merged into engine 1.11.0 (D197 to D199). The nine sets of the regression set render the WAVs and JSON of 1.11.0. stress recording 5, stress recording 7, stress recording 6 and stress recording 8 without stems render the versions of the stage on 1.10.0. On 2026-10-19 the user decided to merge the stage into main.
- 2026-10-20: The stage applies to a device capture (`SourceCondition::device_capture`, `source_condition::reported_device_capture` in the worker plan): a source that is mono, has an anti-phase band, or carries a codec low-pass and arrives at −24 LUFS or less (`source_condition::QUIET_LUFS`) and unlimited (D130). An MP3 or AAC copy of a studio mix keeps the bound of a mix. Basis: user decision of 2026-10-20. The device recordings with only a codec low-pass arrive at −28.32 and −37.98 LUFS; the lossy copies of the regression mixes at −16.22 to −13.28 LUFS, and the lossy demo of regression mix 1 at −18.21 LUFS. stress recording 8 is no longer a device capture. On 1.12.1 the test clip of TODO.md, stress recording 8, and excerpts of stress recording 7 and stress recording 6 give the WAVs of 1.12.1 without stems. Limit: a lossy copy of a studio mix at −24 LUFS or less is a device capture.

## D201: Low band of a captured recording from the range of the album masters

- Recorded: 2026-10-17.
- Origin: TODO.md, "Captured recordings", stage 5 (D146); the capture plan, stage 5. D146 removed the equal-loudness bass move of a capture, and a thin capture kept its low band. The DAW mastering assistant of D200 raises 20 to 60 Hz of these recordings by 0.8 to 7.9 dB.
- Basis: Acoustics, measurement and the pair prior of D177; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented in `capture::low_band`, `capture::LowBand`, `capture::SUB_MINUS_BASS_MINIMUM_DB`, `capture::BASS_MINUS_BODY_MINIMUM_DB`, `broad_tone::Move::CaptureLowBand` and `broad_tone::analyze` as `set.capture-low-band.v1`.
- Decision: For a captured recording whose sub (30 to 80 Hz) against bass (80 to 250 Hz) or bass against body (500 Hz to 2 kHz) lies outside the range of the eight album masters (+0.59 to +5.74 dB and +2.78 to +9.67 dB), stereo low shelves at 80 Hz and 250 Hz, the tops of the sub and bass regions (`capture::SUB_SHELF_HZ`, `capture::BASS_SHELF_HZ`), move the measure to the nearest end of the range, in either direction, by at most 4 dB of shelf gain each (`open::SHELF_MAXIMUM_DB`). The two gains are solved together on the spectrum of the recording, and each shelf moves only toward its own end of the range. A shelf under 0.5 dB is not proposed. The shelves are a correction of `master` and `bold`. They have no tonal gate: the low-end hold counts them as intended moves, and its gate checks the held bands in each section, as for the bass shelves of D182. Their corners differ from the corners of the bass shelves (90 and 300 Hz), so `open::is_bass_shelf` finds only the bass shelves: the sub lift of D204 and the name Full (`version_names::designed`) read those alone.
- Reason: The microphone and the processing of a phone remove the sub: from 76 to 34 Hz the level of the six thin captures falls by 20.6 to 26.6 dB, and that of the nine regression mixes by 1.8 to 17.9 dB. A low shelf restores part of that fall. The album masters give the outcome range of a low end that an engineer chose; a capture inside it keeps its low band.
- Alternatives and tradeoffs: The equal-loudness reference of the low band (+20.4 dB against the body): every stress recording is 9 to 32 dB under it. The median of the album masters as the target: it would move every capture, the bass-heavy stress recording 1 and stress recording 2 included. A larger shelf: it would also raise the rumble and the handling noise of the sub band in the same proportion; the stage cannot separate them from the music there.
- Evidence: the capture stages run. Dry run without stems: stress recording 1, the stress recording 1 excerpt and stress recording 2 lie inside the range and take no shelf. stress recording 6, stress recording 3, stress recording 4, stress recording 5, stress recording 7 and stress recording 8 are 1.75 to 14.47 dB under the smallest outcome of sub against bass, and each takes the 4 dB sub shelf, a predicted move of +1.51 to +2.24 dB; stress recording 8 also takes a bass shelf of −0.92 dB. The DAW mastering assistant at its neutral setting raises the sub against the bass of each of the six by +0.12 to +3.23 dB. The album master of track-f moves its mix from −1.15 to +0.59 dB (+1.75 dB). Renders against 1.9.0, with the corners at 90 and 300 Hz: the sub against the bass of the version with the shelves rises by +1.14 to +2.35 dB (predicted +1.60 to +2.36 dB) and 20 to 60 Hz by +2.38 to +3.34 dB at matched loudness, on stress recording 3 (`bold`), stress recording 4, stress recording 5, stress recording 7, stress recording 6 and stress recording 8 with and without stems. Where the shelf is the only change, 2 to 6 kHz changes by +0.06 to −1.11 dB at matched loudness. stress recording 5 now delivers `master`, which 1.9.0 screened as not distinct. The `bold` of stress recording 7 and stress recording 6 fails the low-end gate by 0.01 to 0.12 dB in its worst section and falls back to a duplicate of `master`; the `open` of stress recording 5 and stress recording 7 is a duplicate of `master`. stress recording 7 delivers two versions against four in 1.9.0.
- Criterion in place of the listening review (pass criterion 7): as for D200, with the range of the album masters in sub against bass and in bass against body. Each of the six thin captures meets it: its sub against bass rises by +1.14 to +2.35 dB and stays under the smallest outcome of the album masters.
- Rule and code references: `set.capture-low-band.v1`; `capture::LOW_BAND_RULE`; `open::regions_db`; the capture stages example.
- Limits and counterexamples: The shelf reaches its bound on each of the six thin captures, so the bound, not the measure, sets their amount. The loudness match takes up to 1.11 dB from 2 to 6 kHz. Two thin captures lose their `bold`, and stress recording 7 its `open`. The stage cannot tell the music in the sub band from rumble or handling noise. On stress recording 3 and stress recording 4, the DAW mastering assistant moves the sub against the bass by +0.22 and +0.12 dB, the stage by about +1.7 dB. The regression mixes track-f, track-d and regression mix 2 also lie under the smallest outcome; they are not captures. The `open` version of a capture keeps its own shelves. With stems, the bass version of D182 adds its shelves on top.
- Verification: `capture::tests::a_thin_sub_takes_a_lift_to_the_album_range`, `capture::tests::a_low_band_inside_the_album_range_takes_no_shelf`, `capture::tests::a_heavy_bass_takes_a_cut_to_the_album_range`, `capture::tests::the_shelves_of_a_capture_are_not_the_shelves_of_the_bass_version`, `broad_tone::tests::a_thin_capture_takes_low_shelves_and_a_mix_does_not`. The nine sets of the regression set render the WAVs and JSON of 1.9.0, and after the merge into 1.10.0 those of 1.10.0. No listening result.
- Review condition: A capture that an engineer mastered; a measure that separates music from noise in the sub band; a listening comparison on stress recording 5 and stress recording 7.
- Work reference: TODO.md, "Captured recordings", stage 5, removed.
- Supersedes / superseded by: Extends D146.

### Updates

- 2026-10-19: The corners of the shelves move from 90 and 300 Hz to 80 and 250 Hz, for the merge into engine 1.10.0. At 90 and 300 Hz with a Q of 1/√2, `open::is_bass_shelf` would read the shelves of a capture as the bass shelves of D182. The sub lift of D204 keeps only rising shelves, so it would drop the bass cut of stress recording 8. With stems it would solve the shelves of the capture and the bass shelves as one pair. A Q of 0.5 at 90 and 300 Hz gave a predicted sub against bass of +0.94 to +1.48 dB on the six thin captures. It also gave bass against body +1.00 to +1.43 dB on five of them, and a bass shelf of −1.99 dB on stress recording 8. At 80 and 250 Hz the dry run gives +1.51 to +2.24 dB and +0.44 to +0.83 dB, and a bass shelf of −0.92 dB on stress recording 8.
- 2026-10-19: Renders on 1.10.0. The sub against the bass of the version with the shelves rises by +1.10 to +2.13 dB, and 20 to 60 Hz by +2.45 to +3.09 dB at matched loudness. Where the shelf is the only change, 2 to 6 kHz changes by −0.03 to −0.87 dB. Each chosen candidate passes every check. The three vivid candidates of the `bold` of stress recording 7 and stress recording 6 fail the low-end gate, at −0.61 to −0.64 dB and at −0.51 dB. stress recording 7 delivers its last candidate, the clear tone with the vivid widening, at 3.8 LU over `master` (D203). stress recording 6 delivers no `bold`, because its last candidate is a described duplicate of `master`. On stress recording 5 the stage `master` carries only the shelves, and its bass step against `essential` is 0.38 dB. The names withdraw it as not distinct, and `open` is a described duplicate of it. stress recording 5 delivers `essential` and `bold`, and stress recording 7 delivers no `open`. The shelves raise the band above the codec cutoff of the stress recording 8 `master` with stems by +0.68 dB, 66.7 dB under 1 to 4 kHz. The nine sets of the regression set render the WAVs and JSON of 1.10.0.
- 2026-10-19: Merged into engine 1.11.0 (D197 to D199). The nine sets of the regression set render the WAVs and JSON of 1.11.0. stress recording 5, stress recording 7, stress recording 6 and stress recording 8 without stems render the versions of the stage on 1.10.0. On 2026-10-19 the user decided to merge the stage into main.
- 2026-10-20: The stage applies to a device capture, as D200 (update of 2026-10-20). The test clip of TODO.md (+2.64 dB at 80 Hz) and stress recording 8 (+4.00 dB at 80 Hz, −0.92 dB at 250 Hz) take no shelf. On 1.12.1 the tone of each version comes from `character::plan` (D206), and no version carries the shelves of the stage; they stay in the broad tone analysis. Basis: user decision of 2026-10-20.

## D202: Compression styles with a section make-up

- Recorded: 2026-10-18.
- Origin: User requirement of 2026-10-18, after the analysis of regression mix 2: "we are trying to create a generic engine though. flattening much more than the reference engineer doesnt sound like a good metric [...] can we not be dynamic about this and do whats best for the track, while having room for different styles among the versions?"
- Basis: Acoustics, measurement and published practice; no listening, owner decision of 2026-10-16.
- Status: Provisional. Implemented as `set.compression.v3`: `compressor::plan`, `compressor::Plan`, `compressor::Style`, `compressor::Settings::section_makeup`, `compressor::PUNCH_ATTACKS_MS`, `compressor::PUNCH_SHARE`, `set::compression_style`, the `compressor` of a Bold version (`set::WorkerVersion`, `set::bold_candidates`), `distributed::PLAN_SCHEMA` (`mastering.worker-plan.8`), `excerpt::CONTEXT_BEFORE_SECONDS` and `excerpt::CONTEXT_AFTER_SECONDS`.
- Decision:
  - Each source that does not arrive limited (D130) takes the compressor. The compressor stays as in `set.compression.v2`: linked 2:1, a 6 dB knee, a 10 ms detector, the threshold over the centred 4 s section level, the release from the drum interval and the detector high-pass from the bass body of the stems. The window-contrast limit of D124 and D125 is removed.
  - A section make-up gives back the energy that the gain reduction removes over the centred 4 s window. Each section keeps the level that it has in the mix, and the compressor changes the dynamics inside a section only.
  - Three styles: Gentle, Punchy and Dense, with a mean gain reduction of 1, 1.5 and 2 dB before the make-up. The attack of Punchy is the slowest of 10, 15, 20, 30, 40, 50, 70 and 100 ms that keeps 95% of the largest punch at 1.5 dB. Gentle takes twice that attack, at most 150 ms. Dense takes half, at least 10 ms. With fewer than 8 onsets, the attack of Punchy is the stem attack or 30 ms.
  - `essential` takes Gentle, `master` and `open` take Punchy, and `bold` takes Dense. The audibility point and the base target come from the prefix of `essential`.
- Reason:
  - The reference of each section level is the mix. The contrast limit measured the compressor alone, and on regression mix 2 it refused each depth (0.70 to 0.71 dB from 1.5 to 0.5 dB), so it acted as a switch. With the make-up, the window contrast of each mix changes by −0.02 to +0.05 dB at each style, and by −0.15 to −0.22 dB on track-d.
  - The attack sets the punch of the onsets and the movement of the gain. The punch is largest at 20 to 70 ms, and the movement falls as the attack rises. The slowest attack near the largest punch keeps the punch and moves the gain less. The search measures the punch on the mix, so the attack needs no stems. The stem attack of D124, the drum decay, is within one step of the largest punch on 8 of 9 mixes.
  - The styles follow the attack regions of practice. A slow attack lets the onsets pass: the slowest attack of a hardware mastering compressor "will prevent most percussive signals from causing limiting or compression", and a mastering textbook gives 50 to 300 ms. A medium attack gives the onsets punch. A fast attack holds the level. The depths are in the range of mastering compression; at 2:1, the punch changes by 0.04 dB or less from 2 to 3 dB.
  - From Gentle to Dense, the punch rises by 0.13 to 0.34 dB and the movement by 0.27 to 0.76 dB. The version closest to the mix compresses least. Bold, the version with the presence and treble steps, compresses most.
- Alternatives and tradeoffs:
  - An attack of 50 ms with the contrast limit (TODO.md, option 2): one fixed attack, and the limit stays a switch for the next mix that fails it.
  - The depth as a fixed share of the short-term range of the mix: at 2:1 and 3 dB the share is 3 to 33% of the range, and track-e stays under 5%.
  - A budget of movement from the modulation threshold of noise, about 0.9 dB from peak to peak at 4 Hz: the compressor that the user preferred in D169 moves the gain by up to 1.23 dB after an onset, and the budget has no listening result on music.
  - A make-up over 12 s: regression mix 2 loses 0.04 to 0.07 dB of window contrast, against 0.00 to 0.02 dB with 4 s.
  - A ratio of 4:1 for Dense: more punch and more movement at the same depth. The styles differ by attack and depth only.
- Evidence: the compression styles run. On the regression set, each source delivers the versions and the names of 1.9.0, except the Full of regression mix 2. Each version of regression mix 2 has a compressor and the 3 ms limiter attack, plays 0.34 LU louder, and its P90 energy attack loss falls from +1.08 and +1.29 dB to +0.01 dB. On the album mixes, the loudness changes by −0.12 to +0.02 LU, and the P90 attack loss by +0.08 to +0.30 dB on `essential`, −0.03 to +0.23 dB on `master` and −0.27 to +0.03 dB on `bold`.
- Rule and code references: `set.compression.v3`; `compressor::plan`, `compressor::process`; the compression rule example.
- Limits and counterexamples: No listening result. One album of eight mixes and regression mix 2. The punch counts each onset of the mix, a vocal onset as a drum onset. Dense moves the gain by up to 1.46 dB after an onset (track-b); a vocal-led master with a heavy kick can dip after each hit (the review condition of D169). The make-up window straddles a step of section level for 2 s: on a synthetic 12 dB step it removes 0.18 to 0.38 dB of window contrast. The compressor lowers the bass of the held bands at matched loudness by 0.43 (Gentle) to 0.74 dB (Dense) on regression mix 2, and the restoration of D160 measures inside passages (D120), so it keeps this change. On regression mix 2 the step of Full against `essential` falls from 0.56 to 0.47 dB, under the 0.5 dB of `set.version-names.v1`, and `master` was not delivered until the sub lift of D204. At matched loudness the compressor gives the limiter more work on 7 of 9 mixes, as in D123. A span render needs 2 s more context on each side.
- Verification: `tests/compressor.rs` (the styles, the make-up, a span with context, the stem timing), the suite, `cargo clippy --all-targets -- -D warnings` and the regression set.
- Review condition: A listening comparison of the three styles; a source whose Dense version moves the gain audibly; a mix with steps of section level over 10 dB.
- Work reference: TODO.md, "User decisions", Compression of regression mix 2, removed.
- Supersedes / superseded by: Supersedes the contrast limit and the depth search of D124 and D125 (`set.compression.v2`), and the stem attack of D124. Uses D130 and D169.

## D203: Bold at the level of a single

- Recorded: 2026-10-18.
- Origin: User decision of 2026-10-18 ("lets go with your suggestion for loudness"), after the user heard the masters as not loud. The compare panel of the site plays each version and the original at the loudness of the quietest one by default (Match loudness).
- Basis: Acoustics, measurement, the published normalization levels and the two masters of track-e by one engineer (D177); no listening.
- Status: Provisional. Implemented as `set.bold-level.v1`: `set::BOLD_AUDIBILITY_MARGIN_LU`, `set::BOLD_TARGET_MAXIMUM_LUFS`, `set::DensityPolicy::bold_target_lufs`, the `target_lufs` of a Bold version (`set::WorkerVersion`, `set::bold_candidates`).
- Decision: `bold` targets 0.5 LU under the audibility point of peak control (D131), at most −6.5 LUFS and at least the base target. A source that arrives limited, or that has no audibility point, keeps the base target. `essential`, `master` and `open` keep the base target of D179, 2.75 LU under the point.
- Reason:
  - Streaming services play music at −14 LUFS (Spotify, YouTube, Tidal, Amazon Music) and −16 LUFS (Apple Music), so the versions for streaming keep the level of the album masters (D179).
  - A player without loudness normalization, a download, a DJ mixer and a club system play a louder file louder. The engineer of D177 delivered the single of track-e at −7.2 LUFS, at its audibility point (−7.27 LUFS), and its album master at −9.2 LUFS.
  - 0.5 LU is the step of D131, and at it the masked NMR of peak control stays under 0 dB on the probes of the point.
  - Bold already carries the Dense compressor and the presence and treble steps; one version for the loud single keeps the other versions for streaming.
- Alternatives and tradeoffs: The level of the single for each version (each version pays the peak control of the single, and streaming turns each down). The Club preset of `todo/presets` (+1.75 LU at −1 dBTP, a request on a version). The audibility point itself (no margin for the point of the Bold chain).
- Evidence: the compression styles run. On the regression set `bold` plays 2.22 to 2.41 LU louder, from −7.69 to −10.67 LUFS. Its P90 attack loss is 2.60 to 5.20 dB, against 2.68 to 4.93 dB for the album masters, and its largest limiter reduction is 0.75 to 3.73 dB. Each set delivers the versions and the names of 1.9.0.
- Rule and code references: `set.bold-level.v1`.
- Limits and counterexamples: The point comes from the prefix of `essential`; the Bold chain has its own. No listening. A louder Bold reaches the low-end gate sooner, so it can take a weaker widening.
- Verification: `tests/set.rs` (the target of Bold and of its trial), the suite, the regression set.
- Review condition: The own audibility point of the Bold chain above its target by less than 0.5 LU; a listening comparison without normalization.
- Work reference: TODO.md, "User decisions", Loudness of the versions, removed.
- Supersedes / superseded by: Changes D179 for `bold`. Answers D155 for one version.

## D204: Sub lift

- Recorded: 2026-10-18.
- Origin: TODO.md, "Sub lift"; user decision of 2026-10-18 ("add the sublift. bring full back while being dynamic").
- Basis: Acoustics and measurement; no listening.
- Status: Provisional. Implemented as `set.sub-lift.v1`: `set::sub_lift`, `set::base_ladder`, `open::lift`, `open::mid_regions_db`, `open::LIFT_MINIMUM_DB`, `selection::one`.
- Decision: A chosen base candidate whose tone carries the bass shelves of the plan (`master` with `set.bass-version.v4`, `open`) renders a second time. The second render measures the mid regions of the first render, the sub against the bass and the bass against the body (`open::mid_regions_db`), against the regions that the tone predicts on the mix. Each shelf takes the shortfall of its region on top of its move (`open::lift`), within the largest outcome of the pairs and 4 dB, and only rises. The version keeps the second render when it passes the gates of the first, and its low-end hold is solved again. A shortfall under 0.1 dB in both regions takes no second render.
- Reason: The chain after the tone takes part of the low end that the shelves give. The compressor turns down the onsets of the kick, which carry the bass, and peak control clips their peaks (D202, D178). The mid holds that low end, and a side gain leaves it.
- Alternatives and tradeoffs: Larger shelves in the plan by a fixed share (the loss changes between mixes). A make-up of the compressor per band (D202: half the loss, at a cost in punch). Two lift passes (one more render for each version with shelves).
- Evidence: the compression styles run. The second render is kept on 14 of 16 versions with shelves; the two of track-e fall short by less than 0.1 dB. The 90 Hz shelf rises by 0.33 to 1.77 dB. The Full of regression mix 2 returns with a bass step of 1.01 dB against `essential`. On the album tracks `master` moves the sub against the bass by +1.42 dB at the median, against +1.38 dB for the album masters and +1.01 dB with 1.9.0, and the bass against the body by +0.93 dB, against +1.17 and +0.61 dB. The CPU time of the regression set rises by 35%.
- Rule and code references: `set.sub-lift.v1`.
- Limits and counterexamples: One more render, and the renders of a hold, for each version with shelves that falls short. The measure is of the whole recording. The second render takes part of its own lift back.
- Verification: `open::tests` (the lift, the outcome bound, the regions), the suite, the regression set.
- Review condition: A second pass where the second render falls short by more than the minimum.
- Work reference: TODO.md, "Album pairs", Sub lift, removed.
- Supersedes / superseded by: Extends D178, D182 and D164.

### Updates

- 2026-10-19: A request on a delivered version renders no sub lift (`set::base_ladder`, `lift`). The worker plan of a request marks its version `lifted` (`adjust::worker_plan`), because the candidates of a request carry the shelves of the parent, sub lift included. With 1.10.0, the base ladder of a request solved the lift again. The Streaming preset on the Full of track-h took the 90 Hz shelf from +3.46 to +4.00 dB, and the band under 120 Hz rose by 0.20 dB at matched loudness. The Radio preset on the Full of track-e took the shelves from +0.90 and +2.09 dB to +1.15 and +2.31 dB, because the lift read the mid gain of the fold as a shortfall of the bass. The versions of a set do not change. Verification: `tests/set.rs`, `a_request_keeps_the_sub_lift_of_its_parent`.

## D205: Temporary storage of the browser engine in host scratch files

- Recorded: 2026-10-20.
- Origin: User requirement of 2026-10-19: lower the peak memory of the browser engine with no loss of speed and no change of the output.
- Status: Accepted. Implemented in `scratch::set_host`, `scratch::temporary` and `HostScratch` of the WASM crate (`e75a766`).
- Decision: On wasm32, `scratch::temporary` takes its storage from the host while a call of the WASM crate runs: a `masteringIO` scratch file, as the renderer of a set already uses (`Sink::scratch_factory`). Without a host it keeps the storage in memory.
- Reason: The 4x prefix of the audibility point of `set::prepare` is the largest buffer of a set: 358 MB for 3.9 minutes, 460 MB for five minutes. In memory it grew a vector by doubling, to 512 MiB for track-e, and the linear memory of a module never shrinks. The WASM set of track-e reached 1,334 MiB of linear memory. iPhone Safari stops a tab at about 1.2 to 1.5 GB.
- Alternatives and tradeoffs: A memory scratch that reserves the whole prefix: 358 MB in place of 768 MiB at the peak, and no host I/O. A prefix that each probe makes again: no storage, and one more upsampling of the recording for each of the up to ten renders of the plan.
- Evidence: the memory run. track-e with its stems: 1,334.3 MiB of linear memory before and 651.5 MiB after. In two pairs, `render_set` took 429.2 and 408.0 CPU-s before and 429.4 and 407.7 CPU-s after.
- Rule and code references: No rule.
- Limits and counterexamples: The host storage holds the prefix during `set::prepare`: 341.2 MiB for track-e, under the 682.4 MiB of the renders. The `audibility::point` of a Club candidate and of an ADJUST request also uses the host storage. A host without scratch files keeps the prefix in memory.
- Verification: The regression set of the eight album mixes gives the WAVs and the JSON of 1.11.0. The WASM set of track-e gives the same four WAVs.
- Review condition: A host that cannot give scratch files, or a storage quota under the scratch storage of a set.
- Work reference: TODO.md, "Render a 22-minute set in the browser".
- Supersedes / superseded by: Extends D145 (the 4x prefix of the audibility point in scratch storage).

## D206: Version character of a single, at the audibility point

- Recorded: 2026-10-20.
- Origin: User requirements of 2026-10-19 and 2026-10-20. The versions must differ: some bold, some essential. Full takes the moves of the single masters. Every version has every stage. A single goes as loud as the safeguards permit.
- Status: Accepted. Implemented in `character`, `width::widening` (`set.widening.v6`), `set::branches`, `set::density` (`set.audible-loudness.v5`) and `selection` (the own-point check).
- Decision:
  - Each version has a `character::Character` (`set.version-character.v1`): a move of each of five regions (low mids, presence, air, sub, bass) and a width of each of six bands.
  - The tone of each version is solved on top of the stem focus, so that the measured moves of the mix reach the moves of the character. A following low-mid move only cuts. When the tone fails `Tone::validate`, the moves scale down in 10% steps.
  - Full takes the moves and the width of the single masters. Essential takes smaller moves and less width. Warm takes more low end, less air and no following low-mid cut. Wide takes the tone of Full and 1 dB more width.
  - Every version has the tone, the sub and bass shelves, the widening, the de-esser, the low-mid cut and the low-end hold.
  - Essential has no reference version. Each other version passes the image gates against Essential.
  - The low-end hold measures the intended move against the shelves of the reference (`low_end::intended_against_db`).
  - Every version, Bold included, sits at the audibility point of peak control (`AUDIBILITY_MARGIN_LU` 0), with a −6.5 LUFS cap. Each version stays at or under the point of its own chain (`Gates::under_own_point`).
  - The own-point check measures the masked NMR of the prepared prefix of the candidate against its render. If the NMR is above 0 dB, a search finds the point of the candidate, and the candidate renders again 0.03 LU under it.
  - The sub lift keeps a cutting shelf, and its solve has a sign. A sub-lift or a hold step that fails `Tone::validate` is not used.
- Reason: The two hot single masters of the reference engineer sit at their audibility points. track-e is at −7.2 LUFS against a point of −7.27 LUFS. The first master of track-h is at −7.1 LUFS against a point of −7.17 LUFS. The streaming services turn a louder master down and add no limiting. The own-point gate stops each version at the level where the limiter and clipper noise becomes audible.
- Alternatives and tradeoffs:
  - A margin of 0.5 LU under the point (`set.audible-loudness.v4`) gives 0.5 LU less level for the same peak control.
  - A full point search for each candidate doubled the render time. The NMR of the render output gives the same value from the render, with no new probe.
- Evidence:
  - The single plan, track-e, with the reference engineer's single master in brackets: low mids −2.28 (−2.14), presence +1.34 (+1.27), air −2.28 (−2.27), sub +1.70 (+1.56), bass −1.26 (−1.20), and width +3.04 (+3.02) dB.
  - track-h: the single master from Tidal is bit-identical to the reference single master. The YouTube Topic upload is the reference single master through a codec. The music video has the first master: −7.1 LUFS, a null of −16.9 dB against the reference single master, presence −1.64, bass −1.05 and width +0.92 dB.
  - Full against the reference single master on track-h: low mids −0.06 (0.00), presence −1.33 (−1.76), air +0.06 (−0.31), sub +1.43 (−0.42), bass −1.08 (−1.42), and width +1.72 (+1.42) dB.
  - The regression set of the eight album mixes delivered each version within 0.1 LU of its point. Examples: track-e −7.26 to −7.27, track-h −7.15 to −7.19, and track-a −8.84 to −8.88 LUFS. Warm sat at the point of its own chain, under the point of the mix, on track-b (−9.38), track-c (−9.90), track-f (−8.74), track-g (−8.25) and track-h (−7.42 LUFS).
  - A set of track-e took 47.5 s alone, and a set of track-a took 98.8 s alone, on the host of the regression.
- Rule and code references: `set.version-character.v1`, `set.widening.v6`, `set.audible-loudness.v5`; `character::plan`, `width::Widening::of`, `selection` (`selection.own_point`, `selection.own_nmr`).
- Limits and counterexamples:
  - The reference engineer cut the sub of track-h by 0.42 dB. Full lifts it by 1.43 dB.
  - The −6.5 LUFS cap did not apply on the corpus.
  - A source that arrives limited keeps the crest rule (`set.source-density.v4`).
- Verification: The unit tests and `tests/set.rs` pass. In the regression set, all eight sets gave four versions after the hold and sub-lift guards. Before the guards, Warm of track-a failed `Tone::validate`.
- Review condition: A pair of a single master that sits under its point, or a listener report of audible limiting on a version at its point.
- Work reference: TODO.md, "Album mode".
- Supersedes / superseded by: Supersedes the margin of D203 and the bass shelves of the branches. Extends D204 (a sub lift with a sign).

### Updates

- 2026-10-20: The point of each version takes the attack loss bound of D208 (`set.audible-loudness.v6`). The rule references of this entry stay those of 1.11.2.

## D207: Running order and album levels

- Recorded: 2026-10-20.
- Origin: User requirement of 2026-10-20: an automatic running order, with the option to drag the tracks into an order of the user. The order needs acoustic checks of each track, a Vinyl option and album levels. A device must master an album without a large increase in memory.
- Status: Accepted. Implemented in `album` (`album.order.v1`, `album.levels.v1`), `set::Options::album_target_lufs`, the `--album-lufs` option of `master set`, the `album_lufs` field of the WASM job, and the WASM functions `measure_track` and `order_album`.
- Decision:
  - `album::measure` makes a summary of each mix: length, loudness, the audibility point, crest, width tier, treble side, tempo and key. It uses the mix only, with no stems.
  - The point comes from the loudest 60 s of the mix, offset by the loudness of that excerpt above the whole track.
  - The tempo is the autocorrelation of the spectral flux, with a felt-tempo weight centered at 120 BPM and half an octave wide.
  - The key is a chroma of the spectral peaks from 55 Hz to 2 kHz, correlated with the Krumhansl profiles.
  - `album::order` scores eight rules. For ten tracks or fewer, it scores each order. Each position gives its reasons and its step from the track before.
  - `album::levels` puts each track at its point. It then lowers the louder of two adjacent tracks to 1 LU above the quieter, or 2.5 LU for a rise into the last track.
  - Each track masters at or under its album target.
  - A host measures one track at a time and keeps only the summaries. The peak memory of an album is the peak memory of one track.
- Reason: The summaries are a few hundred bytes for each track. The order and the levels need only the summaries, so the order page needs no stems and no mastering.
- Alternatives and tradeoffs: The tempo and key of the stems are more accurate, but the stems of an album take longer on a device than the check of the mixes. The order of 2026-10-12 also used the lyrics. The plan does not use the lyrics.
- Evidence:
  - On the eight album mixes, the tempos from the mix matched the tempos from the stems on all eight tracks. The keys matched on five of eight tracks; on the other three, the mode or the fifth was different.
  - The order script of 2026-10-12 gave each key name three semitones too low, because its pitch index started at A. The relations between the keys stay the same.
  - The suggested order of the album mixes is track-h, track-f, track-d, track-g, track-a, track-b, track-c, track-e. The album masters of D177 have the same opener, early peak and closer. In the album masters, two steps of two width tiers come next to track-f, and the rules give these steps a cost.
  - The check took 4.0 to 5.1 s for each mix on the host.
  - track-e with an album target of −9.0 LUFS delivered its four versions from −9.00 to −9.02 LUFS in 51 s.
- Rule and code references: `album.order.v1`, `album.levels.v1`; the album tests, the album order example.
- Limits and counterexamples:
  - The point of the check comes from the mix with no stem timing. It can be different from the point of the set: track-c is at −9.74 LUFS in the check and −8.77 LUFS in the set.
  - A track whose own point is under its album target masters at its own point.
  - Local mastering accepts tracks of five minutes or less.
- Verification: The unit tests of `album` and the album tests pass.
- Review condition: An album order of a mastering engineer that the rules score higher than the suggested order, or a key error of the check that changes an order.
- Work reference: TODO.md, "Album mode"; the album plan.
- Supersedes / superseded by: Extends D177 (the album masters).

### Updates

- 2026-10-20: `album.order.v2` and `album.levels.v2`, after two reviews of the weights and a test of the levels. Each track within 0.25 dB of the lowest crest counts as the densest. A key that leads its second key by less than 0.05 in correlation is not clear, and its step takes the relation of the lower cost of the two keys. The width step and the tempo step break ties only. On the album mixes, v2 gives track-h, track-g, track-f, track-d, track-c, track-a, track-b, track-e: positions 1 and 4 to 8 are those of the album masters, and positions 2 and 3 change places. The keys of track-a and of track-g are not clear. A level cut stays within 1 LU of the point of a track. A mastered track keeps its delivered loudness, and the levels of the other tracks follow it. Each level gives its step from the track before. The WASM order job takes the delivered loudness of the mastered tracks (`delivered`), and each track summary gives `key_clear` and `second_key`. Verification: the album tests. The reviews read this album, so it is not an independent test of v2.

## D208: Attack loss bound of the audibility point

- Recorded: 2026-10-20.
- Origin: User decisions of 2026-10-20. A single goes as loud as the safeguards permit. Of the three settings offered, the user chose the attack loss bound alone, with the ratio of the loudest frames reported and not enforced.
- Status: Accepted. Implemented in `audibility` (`ATTACK_LOSS_MAX_DB`, `Probe::of`, `Point`), `selection` (`selection.own_point`, `selection.own_loud_nmr`) and `distributed` (the reference record), as `set.audible-loudness.v6`.
- Decision:
  - The audibility point is the loudest output at which the total masked noise-to-mask ratio of peak control is at most 0 dB and the P90 attack loss against the mix is at most 5.45 dB.
  - 5.45 dB is the P90 attack loss of the first master of track-h, at −7.1 LUFS, the hottest master of the pairs. The album masters of D177 lose 2.68 to 4.93 dB.
  - The search of the point reads each probe as the larger of its ratio and ten times its attack loss over the bound. The ratio rises 4 to 5 dB per LU and the attack loss about 0.45 dB per LU (D179), so one tolerance holds both.
  - Each probe and each candidate reports the ratio that the loudest 5% of the frames reach (`loud_nmr_db`). The report compares it with the PEAQ threshold of a disturbed frame, 1.5 dB. The point does not use it.
  - The reference record of a distributed set carries the spec that Essential chooses. The fallback to the instant attack renders the reference of the native set.
- Reason: The total ratio is an average over the recording. A loud chorus over the masking threshold balances quiet verses under it. The gain fit of the disturbance removes the gain riding of the limiter, and the attack loss measures it. At the point of the ratio alone, three mixes lost more attack than any master of the pairs.
- Alternatives and tradeoffs:
  - The attack loss bound and a bound of the loudest frames at 1.5 dB, with the attack loss at 5.45 dB: track-b, track-c, track-d, track-e and track-f come down 1.0 to 1.5 LU more.
  - Both bounds with the attack loss at 4.93 dB, the largest loss of the album masters: track-h sits at −8.30 LUFS, the quietest of the settings.
  - No bound, with both measures reported: the masters keep the loudness of 1.11.2.
- Evidence:
  - The track-h masters, measured against the mix: the first master at −7.1 LUFS loses 5.45 dB, the second master at −8.3 LUFS loses 4.35 dB, and the Opus copy of the first master on YouTube at −8.3 LUFS loses 3.78 dB.
  - The audibility point example (`examples/audibility_point`) on the eight album mixes, with the ratio alone: track-a at −8.08 LUFS loses 7.12 dB, track-g at −6.87 LUFS loses 7.25 dB, track-h at −6.58 LUFS loses 6.80 dB, track-f at −7.76 LUFS loses 6.05 dB, track-d at −7.41 LUFS loses 5.54 dB, track-c at −8.21 LUFS loses 5.47 dB, track-b at −8.24 LUFS loses 5.06 dB and track-e at −6.60 LUFS loses 3.70 dB. The loudest 5% of the frames reach 4.83 to 6.14 dB on every mix.
  - With the bound at 5.45 dB, the bound sets the point of six mixes: track-a at −9.41 LUFS, track-f at −8.25 LUFS, track-g at −8.33 LUFS, track-h at −7.83 LUFS, track-d at −7.56 LUFS and track-c at −8.28 LUFS. The ratio sets the point of track-b and of track-e.
- Rule and code references: `set.audible-loudness.v6`; `audibility::ATTACK_LOSS_MAX_DB`, `audibility::Probe`, `selection::own_point`, `distributed::Reference`; the audibility point example, `tests/set.rs`.
- Limits and counterexamples:
  - The bound comes from one master of one mix.
  - The ratio of the loudest 5% of the frames of the versions ranges from −4.2 to +8.0 dB. It is over the PEAQ threshold of 1.5 dB on every version of track-b, track-c, track-e and track-f, and under it on Bold of track-a, track-g and track-h.
  - A source that arrives limited keeps the crest rule (`set.source-density.v4`).
- Verification: `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass (288 tests). The regression of the eight album sets is under "Updates".
- Review condition: A single master of the pairs with a P90 attack loss over 5.45 dB, or a listener report of audible limiting on a version at its point.
- Work reference: TODO.md, "Next" (the loudest frames).
- Supersedes / superseded by: Extends D206 (the point of each version). `set.audible-loudness.v6` replaces `set.audible-loudness.v5`.

### Updates

- 2026-10-20: Regression of the eight album sets, engine 1.11.2 against 1.13.0, each set with its stems. Every set delivers four versions. The P90 attack loss of every version is at most 5.45 dB; on 1.11.2 it reached 7.16 dB (Essential of track-g). track-b and track-e keep their loudness to 0.01 LU. Essential comes down by 1.52 LU on track-a (−8.88 to −10.40 LUFS), 1.78 LU on track-g (−7.64 to −9.42 LUFS), 1.19 LU on track-h (−7.19 to −8.38 LUFS), 0.51 LU on track-c, 0.44 LU on track-f and 0.24 LU on track-d. Full comes down by 1.04 LU on track-a, 1.50 LU on track-g, 1.06 LU on track-h, 0.44 LU on track-f, 0.17 LU on track-d and 0.14 LU on track-c. Bold follows Full within 0.1 LU. Open comes down by 1.81 LU on track-a, 1.47 LU on track-g, 1.27 LU on track-h and 0.57 LU on track-d. The set of track-e in the browser engine (Node) gives the four WAVs of the native set and peaks at 409.5 MiB of linear memory.

## D209: Keyed de-essing from the 90% excess of the recording

- Recorded: 2026-10-21. Revised 2026-10-24.
- Origin: Owner decision of 2026-10-21: a de-esser is a rule-based stage, and the engine applies it by rule. The album masters of D177 stay as evidence and set no default. The decision is in the de-esser plan. Owner decisions of 2026-10-23: the measurement decides, with no listening check; after the sweep of that date, the default has no cut, and the switch on cuts from the 90% quantile of the excess by up to 3 dB, scaled by the vocal share, with the stem check.
- Basis: Measurement and the pair prior of D177; no listening, owner decisions of 2026-10-21 and 2026-10-23.
- Status: Provisional. Implemented in `deesser::key_with_frames`, `deesser::Plan`, `deesser::THRESHOLD_QUANTILE`, `deesser::MAXIMUM_CUT_DB`, `deesser::OTHER_LOSS_LIMIT_DB`, `deesser::THRESHOLD_RANGE_DB`, `deesser::CUT_LIMIT_DB`, `deesser::Plan::delivered`, `set::Options::deesser` and `adjust::sibilance_shift` as `set.deesser.v4`. The `--deesser` option of `master set`, the `deesser` field of the WASM job and the `deesser` field of a studio submit set the switch. Not deployed: the site runs engine 1.13.0 and the cloud runs engine 1.9.0.
- Decision:
  - Key: the key of D133 and D153 stays. A frame is also keyed only when the mid of the vocal stem has more 5 to 10 kHz power than the mid of each of the drum, bass and other stems in that frame.
  - Each keyed frame carries its vocal share: the 5 to 10 kHz mid power of the vocal stem over that of the mix, at most 1. It also carries the 5 to 10 kHz mid power of the drum, bass and other stems in that frame.
  - Reference excess: the 90% quantile of the excess of the 5 to 10 kHz band over the 1 to 4 kHz band of the mid over the keyed frames of the render (`excess_quantiles_db[18]`, `deesser::THRESHOLD_QUANTILE`). The threshold is the reference excess plus `threshold_db` of the plan (0 dB in a set).
  - Cut: a keyed frame with an excess above the threshold is cut by the excess over the threshold, times its vocal share, by up to the largest cut of the plan. Every version of a set carries the plan.
  - Switch (`set::Options::deesser`): off by default. With the switch off, or with the field absent or null, the plan keeps the key and the measure, with a largest cut of 0 dB, as in D181. With the switch on, the largest cut of the versions of a set is 3 dB (`deesser::MAXIMUM_CUT_DB`).
  - Stem check: when the cut removes more than 0.3 dB of the 5 to 10 kHz mid energy of the drum, bass and other stems over the recording, the render halves the largest cut once. When the half cut also fails, the render cuts nothing. The render report gives the result (`stem_check`).
  - Fixed reference: a render from a delivered version (a preview span or a request) uses the reference excess and the largest cut of the delivered render and runs no stem check. A span then equals the same frames of the whole render, and a request moves from the delivered cut.
  - Sibilance control: a step down lowers the threshold and raises the largest cut by the same shift. The threshold stays from 12 dB under to 6 dB over the reference excess (`deesser::THRESHOLD_RANGE_DB`), and the largest cut stays at or under 12 dB (`deesser::CUT_LIMIT_DB`). A step up raises the threshold and lowers the largest cut. A step up larger than the remaining mean cut takes the cut to 0 dB, with the threshold at most 6 dB over the reference excess. A version without a cut offers no step up.
- Reason: The album masters show no keyed cut beyond v3 (D177, D181), and no setting of the sweep with a cut holds them in their range, so the default cuts nothing. With the switch on, the 90% quantile cuts the loudest tenth of the keyed frames of the recording, so a vocal chain that holds its sibilance gets a small cut. The band filter cuts every source of the band in a frame by the same gain, so the vocal share and the stem check bound the loss of the other sources. The reference excess is 3.8 to 10.8 dB over the median excess on the run corpus, so a step down goes to 12 dB under it: four steps down then have their mean cut on every mix.
- Alternatives and tradeoffs: The median as the reference with a cut by default (the plan of 2026-10-21): it cut half the keyed frames and put the sibilant P99 of `master` 2.21 to 4.28 dB under the other vocal frames. A setting that holds the album range: in the sweep, every setting that cuts fits 4 of 16 mix and version pairs or fewer. The cut of D133, at a threshold of 0 dB and up to 6 dB, put the sibilant P99 of `master` 4.8 to 6.6 dB under the other vocal frames. A largest cut of 6 dB: the depth question of D153. A Sibilance range of 6 dB under the 90% quantile: four steps down at the full share reach 1 of 9 mixes.
- Evidence:
  - Untuned rule: the measurement run of 2026-10-21 of the median rule with 3 dB, on the working tree of `set.deesser.v4` on `2cbcd54`, local, with the eight confirmation mixes and regression mix 2, in the de-esser v4 run. The v4 key holds the same frames as v3 on all nine mixes. The stem check passes on 36 of 36 versions, with `other_loss_db` −0.00002 to −0.00893 dB. The sibilant P99 of `master` lands 2.21 to 4.28 dB under the other vocal frames and 2.21 to 4.39 dB under the album master on the seven tracks that have one. The plan expected the P99 between v3 and the album masters: not met on 16 of 16 mix and version pairs. The eight mixes without their vocals key no frame with v3 and v4. The switch off gives WAV files byte-identical to v3 on nine of nine mixes. The v4 sets pass the selection gates and keep the versions and the chosen candidates of v3; the names of the track-g set change.
  - Sweep of 2026-10-23, in the de-esser v4 run and its sweep file: the threshold at the 50%, 65%, 75%, 85%, 90% and 95% quantiles, with largest cuts of 1, 1.5, 2 and 3 dB, on `master` and `bold` of the eight confirmation mixes, from the delivered state of the sets with the switch off. The sweep reproduces the untuned v4 sets within 0.0026 dB and the v3 sets within 2.3e−6 dB. The P99 falls by about the largest cut at each quantile. The stem check passes on 384 of 384 renders with a cut, with `other_loss_db` −0.00893 dB or higher.
  - Album range scores: a pair fits when P99 − other is within −0.84 to +0.11 dB, or within 0.25 dB under v3 where v3 is already under −0.84 dB. No cut: 16 of 16. The 95% quantile up to 1 dB: 4 of 16. The 90% quantile up to 1 dB: 2 of 16. The median up to 1 dB: 1 of 16. The other settings with a cut: 0 to 2 of 16. The 90% quantile up to 3 dB: 0 of 16.
  - Final rule, measured 2026-10-24 with 18 full set renders of the nine mixes, in the de-esser v4 run. Default: the WAV files and `index.html` are byte-identical to v3 on 54 of 54 files, and every plan has a largest cut of 0 dB. Switch on: each version cuts 9.9% to 10.1% of its keyed frames, with a mean cut of 0.128 to 0.242 dB over the keyed frames and a largest cut of 3.00 dB. P99 − other is −3.70 to −1.87 dB for `master` and −3.96 to −1.86 dB for `bold`, 0 of 16 pairs in the album range; the other vocal frames move by +0.0047 dB or less. The cut frames equal the sweep on 16 of 16 pairs. The stem check passes on 36 of 36 versions, with `other_loss_db` −0.00138 dB or higher. The key equals the run and the v3 sibilant frames on 9 of 9 mixes. On regression mix 2, 6 to 12 kHz of `master` against `essential` is +0.705 dB (v3 +0.702 dB), and the switch lowers 6 to 12 kHz of each version by 0.170 to 0.182 dB. The sets pass the selection gates and keep the versions, the order, the names and the chosen candidates of v3 on 9 of 9 sets; delivered loudness moves by −0.0123 to +0.0102 LU.
  - Sibilance range, from the 72 version reports of the run: with the threshold at most 6 dB under the 90% quantile, four steps down add at most 0.97 to 2.33 dB of mean cut. With 12 dB and a largest cut of at most 12 dB, they add at most 3.08 to 6.97 dB. Four steps need 2 dB at the full share.
- Rule and code references: `set.deesser.v4`; `deesser::THRESHOLD_QUANTILE`, `deesser::MAXIMUM_CUT_DB`, `deesser::OTHER_LOSS_LIMIT_DB`, `deesser::THRESHOLD_RANGE_DB`, `deesser::CUT_LIMIT_DB`, `deesser::StemCheck`, `set::Options::deesser`, `adjust::mean_cut`, `adjust::sibilance_shift`, `excerpt::delivered_spec`.
- Limits and counterexamples:
  - With the switch on, the sibilant P99 of every pair lands 1.51 to 2.98 dB under v3 and outside the album range.
  - The limit of the stem check (−0.3 dB) stays provisional. On the run corpus, a loss of all drum, bass and other band power in every keyed frame gives −0.0019 to −0.0981 dB, so the gate does not act on these mixes.
  - The cut applies to every version of a set, `essential` and `open` included.
  - A Sibilance step of −4 can cut the loudest frames by up to 12 dB.
  - The app sends no `deesser` field yet, and its switch is hidden (`bitneedle-app` `59f1ec5`).
- Verification:
  - Tests: the tests of `deesser`, `adjust::tests::a_sibilance_step_up_takes_the_cut_down_to_none`, `adjust::tests::four_sibilance_steps_down_reach_a_wide_distribution`, `excerpt::tests::a_span_equals_the_same_frames_of_the_whole_render` and `the_de_esser_switch_of_a_submit_is_a_boolean` of the control Lambda. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --release` pass.
  - Measurement: the run, the sweep and the measurement of the final rule, in the de-esser v4 run.
  - Listening: none. The owner decision of 2026-10-23 removes the listening check.
- Review condition: New album pairs whose masters show a measured sibilance reduction beyond v3 (P99 − other under the album range); a harsh vocal in a set with the switch off; a render whose stem check halves or removes the cut.
- Work reference: the de-esser plan; the de-esser v4 run; TODO.md, "De-esser v4".
- Supersedes / superseded by: `set.deesser.v4` replaces `set.deesser.v3` (D181). The default of D181, no cut in the versions of a set, stays in force through this decision.

## Entry template

```text
## Dnnn: Subject

- Recorded: YYYY-MM-DD.
- Origin: User requirement, source observation, engineering proposal, or experiment.
- Status: Proposed | Accepted | Provisional | Rejected | Superseded.
- Implementation: Absent, experimental, or implemented; link the relevant revision.
- Decision or proposal:
- Reason:
- Alternatives and tradeoffs:
- Evidence: Source title, speaker, timestamp, context, or measurement record.
- Rule and code references:
- Limits and counterexamples:
- Verification: Test results and listening results, reported separately.
- Review condition:
- Work reference: TODO section.
- Supersedes / superseded by: Entry ID, if applicable.

### Updates

- YYYY-MM-DD: Evidence, correction, or status change; reason and links.
```

## D210: Wavey Artist Source Licence for the engine

- Recorded: 2026-10-25.
- Origin: Owner decision of 2026-10-25. The engine takes the licence of the bitneedle record-authoring crates, with its terms unchanged.
- Basis: Owner decision.
- Status: Accepted. Implemented in `LICENSE`, `Cargo.toml` (`license-file`) and the public export list. Replaces the MIT licence of 2026-10-25, which the public repo carried for one commit.
- Decision: The Wavey Artist Source Licence (version 0.1) applies to the repository and to the public repo `wavey-ai/mastering`. Artists and Artist Entities can use the engine to master and sell their own music. Any person can study, run and modify the engine for personal experimentation, education, evaluation or non-commercial research. A label, platform, service provider or other commercial user needs a separate licence from licence@yl.vin.
- Reason: The engine is for the artist who masters their own music. A commercial mastering service or a label workflow needs a written licence.
- Alternatives and tradeoffs: The MIT licence allows any commercial use, including a hosted mastering service built on the engine.
- Limits: The licence text names Bitneedle in its contractor and trademark sections; the terms apply to the engine as written.
