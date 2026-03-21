# Missing features
- Personas
- PR/MR
- Inboxes

# Bugs / lazy faults
- CORS failure - composer 2 had set up env vars for CORS allowed origins but hadn't set up the CORS layer middleware
    - CORS heresy
- Git provider sign in hidden away on the 'api playground' page instead of somewhere prominent for users to know its important
- No dotenvy or other developer QoL setup around env vars
- Nowhere to seta agent cli token in the ui (playground page again)
- Auth status not prominent
- Didn't bundle agent clis into docker
- Referencing spec docs in the UI
- Not passing force or yolo to agents
- Not passing the model through from the session to the agent
- Not using meaningful branch names or commit messages
- CLI / UI Parity on wake integration

# Interestig notes
- Chose postgres