# Premise
Starting from the same basis (docs and prompts), see how composer 2 and opus 4.6 compare. Harness engineering effort is intentionally low

# Qualitative outcome
Both models missed major features, had a variety of minor bugs, and failed to set up environments correctly (e.g. installing agent clis in the docker containers). 

Data collected (manually and by agents) is available in ISSUES_LOG.md and AGENT_REVIEW.md. Taking a vibe's based assessment approach, I 'feel' like composer-2 had more minor / lazy bugs but fewer major features missed, whereas opus code quality and alignment was better with more major features missed. Composer-2 definitely seemed faster.

Overall I'd say the mjor contributing factor is the quality of the spec and the harness and that both models are quite good, even comparable

# Notes
Not all features have been tested end to end. Manual effort was applied to get to the point where one end to end flow was working. This has given me the data I wanted on the models, further engineering effort required to drive the desired outcome