import { Linking, Share, Alert } from 'react-native';
import { P2POffer, LoanOrder } from '../types';

export const CLOCKLEND_ACTION_BASE = 'https://clocklend.xyz/api/actions/clocklend';

/**
 * Generate a standard Solana Action URL for a P2P pawn listing
 */
export function getPawnBlinkUrl(offerId: number): string {
  return `solana-action:${CLOCKLEND_ACTION_BASE}/pawn/${offerId}`;
}

/**
 * Generate a standard Solana Action URL for a 24h grace rescue buyout
 */
export function getGraceRescueBlinkUrl(orderId: number): string {
  return `solana-action:${CLOCKLEND_ACTION_BASE}/rescue/${orderId}`;
}

/**
 * Share a P2P Pawn listing into TARDIS (or system share sheet fallback)
 */
export async function sharePawnToTardis(offer: P2POffer): Promise<{ shared: boolean; method: 'tardis' | 'system' }> {
  const blinkUrl = getPawnBlinkUrl(offer.id);
  const text = `🤝 [ClockLend Pawn #${offer.id}] Need $${offer.requestedAmount} USDC against ${offer.collateralName} (${offer.durationDays}d). Earn +$${offer.interestOffered} USDC yield!\n\n${blinkUrl}`;

  const tardisDeepLink = `tardisapp://post?content=${encodeURIComponent(text)}`;

  try {
    const canOpenTardis = await Linking.canOpenURL('tardisapp://');
    if (canOpenTardis) {
      await Linking.openURL(tardisDeepLink);
      return { shared: true, method: 'tardis' };
    }
  } catch (err) {
    console.warn('Tardis deep link check failed, falling back to Share sheet:', err);
  }

  // Fallback to Native Share Sheet (works on all Android/iOS devices)
  try {
    await Share.share({
      title: `ClockLend Pawn #${offer.id}`,
      message: text,
      url: blinkUrl,
    });
    return { shared: true, method: 'system' };
  } catch (err) {
    console.warn('Share sheet error:', err);
    return { shared: false, method: 'system' };
  }
}

/**
 * Dispatch an emergency 24-Hour Social Grace rescue request into TARDIS
 */
export async function requestTardisGraceRescue(
  order: LoanOrder
): Promise<{ dispatched: boolean; method: 'tardis' | 'system' }> {
  const rescueBlinkUrl = getGraceRescueBlinkUrl(order.id);
  const text = `🚨 [ClockLend Peer Rescue] My loan #${order.id} in ${order.poolName} entered its 24h grace window! Circle peers have priority buyout rights to claim ${order.collateralName} collateral:\n\n${rescueBlinkUrl}`;

  const tardisDeepLink = `tardisapp://post?content=${encodeURIComponent(text)}`;

  try {
    const canOpenTardis = await Linking.canOpenURL('tardisapp://');
    if (canOpenTardis) {
      await Linking.openURL(tardisDeepLink);
      return { dispatched: true, method: 'tardis' };
    }
  } catch (err) {
    console.warn('Tardis deep link failed, using Share sheet:', err);
  }

  try {
    await Share.share({
      title: `🚨 Emergency ClockLend Peer Rescue (#${order.id})`,
      message: text,
      url: rescueBlinkUrl,
    });
    return { dispatched: true, method: 'system' };
  } catch (err) {
    console.warn('Share sheet error:', err);
    return { dispatched: false, method: 'system' };
  }
}

/**
 * Open a direct encrypted DM with a counterparty inside TARDIS
 */
export async function openTardisDm(skrHandle: string): Promise<boolean> {
  const clean = skrHandle.replace('@', '').trim();
  const deepLink = `tardisapp://dm/${clean}`;

  try {
    const canOpen = await Linking.canOpenURL('tardisapp://');
    if (canOpen) {
      await Linking.openURL(deepLink);
      return true;
    }
  } catch (err) {
    console.warn('Could not open TARDIS DM:', err);
  }

  Alert.alert(
    'TARDIS Messenger',
    `Counterparty handle: @${clean}\n\nOpen the TARDIS app to send an end-to-end encrypted hardware message.`
  );
  return false;
}

/**
 * Open a Gated Community Circle inside TARDIS
 */
export async function openTardisCommunity(communityId: string): Promise<boolean> {
  const deepLink = `tardisapp://community/${communityId}`;

  try {
    const canOpen = await Linking.canOpenURL('tardisapp://');
    if (canOpen) {
      await Linking.openURL(deepLink);
      return true;
    }
  } catch (err) {
    console.warn('Could not open TARDIS community:', err);
  }

  Alert.alert(
    'TARDIS Circle',
    `Community Circle: ${communityId}\n\nJoin this group inside TARDIS to unlock community member rates.`
  );
  return false;
}
