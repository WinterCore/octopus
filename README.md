# Oct*opus*


## The plan is to build a radio app that live streams [opus](https://opus-codec.org/) audio files over http.


### TODO
- [ ] Fix bug with initial audio buffering to clients. eg: Newly connected clients should always receive at least 4 seconds of audio instantly so that the browser can play the audio right away.
- [ ] I'm not sure if it's possible to send audio data without overstreaming, I think there's a bug with the sleep time calcluation, if that doesn't work then I might need to decode/encode the opus data which will give me more control.


### MVP Features
- [x] Implement basic http audio playback.
- [ ] Build an admin web UI
- [ ] Build a client web UI
