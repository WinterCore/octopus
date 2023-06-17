<script lang="ts">
    import Progress from "./progress.svelte";

    export let progress: number;
</script>

<div class="w-full max-w-[400px] px-2">
    <Progress strokeWidth={4}
              time={155}
              totalTime={155}
              class="w-full h-full" />
</div>
