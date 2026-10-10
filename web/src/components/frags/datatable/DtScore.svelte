<script lang="ts">
	import type { ResponseEntry } from '#lib/types/ResponseEntry.js';
	import { getScoreVariant, isCambiaResponse } from '#lib/utils.js';

	interface Props {
		res: ResponseEntry;
	}

	let { res }: Props = $props();
	let score = $derived(
		res.content && isCambiaResponse(res.content) && res.status === 'processed'
			? res.content!.evaluation_combined.filter((x) => x.evaluator === 'OPS')[0].combined_score
			: 'N/A'
	);
</script>

{#if score}
	<div class="flex chip {getScoreVariant(score)} rounded-full pointer-events-none">{score}</div>
{/if}
