<script lang="ts">
    import Progress from "./progress.svelte";
    import PlayIcon from "../icons/play.svelte";
    import {onDestroy} from "svelte";

    let time = 20;
    let totalTime = 500;

    const progressInterval = setInterval(() => time += 20, 1000);

    onDestroy(() => clearInterval(progressInterval));
</script>

<div class="w-full max-w-[400px] px-2">
    <h1 class="text-center text-white text-3xl">
        Song name
    </h1>
    <h2 class="text-center text-white/70 text-lg mt-1">
        Author Smith
    </h2>
    <Progress strokeWidth={4}
              time={155}
              totalTime={155}
              audioMetaData={{
                name: "Potato",
                author: undefined,
                image: "https://encrypted-tbn0.gstatic.com/licensed-image?q=tbn:ANd9GcRisdJNBLznLQcXdxtaD4GSBYCWaviucAhaOpoe9tt_yC7kzGvGHtjtGMfdYynM3x-ImyQ1UjgcnmyJ1WQ",
              }}
              progress={{
                currTime: time,
                totalTime,
              }}
              class="w-full h-full mt-4" />

    <div class="flex justify-center mt-12">
        <PlayIcon class="w-16 h-16 text-white hover:scale-[1.04] transform-gpu" />
    </div>
</div>
