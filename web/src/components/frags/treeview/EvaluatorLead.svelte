<script lang="ts">
	import type { EvaluatorType } from '#lib/types/EvaluatorType.js';
	import IconUnknown from '~icons/carbon/unknown';
	import Orpheus from '../../icons/Orpheus.svelte';
	import type { IconComponent } from '#lib/types/IconComponent.js';

	interface Props {
		evaluator: EvaluatorType;
	}

	let { evaluator }: Props = $props();

	const Icon: IconComponent = $derived(evaluator === 'OPS' ? Orpheus : IconUnknown);
</script>

<div class="w-6">
	<Icon />
</div>
